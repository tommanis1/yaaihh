//! The Policy page in the Settings page's look: the levels listed on the left, each fact the package resolves level by
//! level a row on the right, with its value at the level shown and the package's act that sets it there.

use super::ask::{resolve, Ask, Resolved, Said};
use super::plan::{Fill, Level, Plan, Row};
use crate::settings_look::{self as look, faint_value};
use crate::v2::desk::Refusal;
use rune::{col, text, El};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::Value as Json;

/// A fact's name without the module it is named through.
fn short(fact: &str) -> &str {
    fact.rsplit('.').next().unwrap_or(fact)
}

/// A fact as the page names it: yaaihh's own in the Settings page's words, any other by its name.
fn plain(fact: &str) -> &str {
    match short(fact) {
        "Mode-Of" => "Permissions",
        "Agent-Mode" => "the agent",
        "Project-Mode" => "the project",
        "Install-Mode" => "this computer",
        s => s,
    }
}

/// Where a value set at level L comes from, as a row says it.
fn from(l: &Level) -> String {
    if l.per_party { "set for the agent".into() } else { format!("from {}", plain(&l.fact)) }
}

/// A level the page shows: one that holds for everyone, by its fact, or one party's own.
#[derive(Clone, Debug, PartialEq)]
pub enum At {
    Everyone(String),
    Party(String),
}

impl At {
    fn key(&self) -> String {
        match self {
            At::Everyone(f) => format!("policy-at-{f}"),
            At::Party(p) => format!("policy-at-party-{p}"),
        }
    }

    fn name(&self) -> String {
        match self {
            At::Everyone(f) => word(plain(f)),
            At::Party(p) => p.clone(),
        }
    }
}

/// How the page changes a level: the party the window acts as, and what sends an act for it.
pub struct Acts<S> {
    pub me: String,
    pub send: fn(&mut S, &str, Vec<Json>),
}

/// What the page reads of the engine: its `loaded` hash once that line came, its parties, its answers, the record's
/// refusal of a level's acts, the act it would not take at all, how to act (none: the page only shows), the level
/// shown (none: the most specific one for everyone) and what a press on a level does.
pub struct Engine<'a, S> {
    pub loaded: Option<&'a str>,
    pub parties: &'a [String],
    pub ask: Ask<'a>,
    pub refused: &'a dyn Fn(&[&str]) -> Option<Refusal>,
    pub no: Option<&'a str>,
    pub acts: Option<Acts<S>>,
    pub at: Option<&'a At>,
    pub go: fn(&mut S, At),
}

/// The levels to pick from: each that holds for everyone, most general first, then each party.
fn places(plan: &Plan, parties: &[String]) -> Vec<At> {
    let mut v: Vec<At> = Vec::new();
    for l in plan.rows.iter().flat_map(|r| &r.levels).filter(|l| !l.per_party) {
        let at = At::Everyone(l.fact.clone());
        if !v.contains(&at) {
            v.push(at);
        }
    }
    v.extend(parties.iter().map(|p| At::Party(p.clone())));
    v
}

/// The page for PLAN on the engine E.
pub fn page<S: 'static>(plan: &Plan, e: &Engine<S>, k: &Kit) -> El {
    let places = places(plan, e.parties);
    let general = places.iter().rev().find(|a| matches!(a, At::Everyone(_))).or(places.first());
    let at = e.at.filter(|a| places.contains(a)).or(general).cloned();
    let mut menu = look::levels("policy-levels");
    for (i, a) in places.iter().enumerate() {
        if matches!(a, At::Party(_)) && (i == 0 || matches!(places[i - 1], At::Everyone(_))) {
            menu = menu.child(look::heading("Parties", k));
        }
        let (go, to) = (e.go, a.clone());
        menu = menu.child(look::level(a.key(), &a.name(), at.as_ref() == Some(a), move |s: &mut S| go(s, to.clone()), k));
    }
    let mut body = col().w_full().child(head(plan, e, k));
    if plan.rows.is_empty() {
        body = body.child(text("The package resolves no fact level by level.").font(k.ty.body).color(k.c.text_muted).pt(12.0));
    }
    if let Some(at) = &at {
        for r in &plan.rows {
            body = body.child(row_el(r, at, e, k));
        }
    }
    look::page("policy-v2", menu, &at.as_ref().map_or("Policy".into(), At::name), body, k)
}

/// What the page reads and does, whether the engine runs the text it read, and an act the engine would not take.
fn head<S>(plan: &Plan, e: &Engine<S>, k: &Kit) -> El {
    let how = if e.acts.is_some() { "each set by the package's own act" } else { "read-only here" };
    let said = text(format!("Asked of the engine now, {how}.")).font(k.ty.small).color(k.c.text_faint);
    let mut c = col().gap(2.0).child(said).child(same_text(&plan.hash, e.loaded, k));
    if let Some(no) = e.no {
        c = c.child(text(format!("The engine would not take {no}")).font(k.ty.small).color(k.c.danger.fg).key("policy-no"));
    }
    c
}

/// Whether the engine runs the text the page read: its `loaded` line's hash against the page's own.
fn same_text(hash: &str, loaded: Option<&str>, k: &Kit) -> El {
    let short = |h: &str| h.chars().take(12).collect::<String>();
    let t = match loaded {
        None => text("The engine's loaded line has not come yet.").color(k.c.text_faint),
        Some(h) if h == hash => text(format!("The text the engine runs (loaded {}).", short(h))).color(k.c.text_faint),
        Some(h) => text(format!("The engine runs another version ({} vs {}).", short(h), short(hash))).color(k.c.warning.fg),
    };
    t.font(k.ty.small).key("policy-text")
}

/// What a fact says when it is not one value.
fn said_el(s: &Said, k: &Kit) -> El {
    match s {
        Said::Refused(why) => text(format!("refused: {why}")).font(k.ty.small).color(k.c.danger.fg).truncate().shrink(),
        Said::Holds(v) if v.len() > 1 => text(format!("{} all hold", v.join(", "))).font(k.ty.body).color(k.c.warning.fg),
        _ => text("…").font(k.ty.body).color(k.c.text_faint),
    }
}

/// The value the levels before I give, and where from: the most specific one set, else the package's default; None
/// while one of them waits for its answer.
fn above(r: &Row, res: &Resolved, i: usize) -> Option<(Option<String>, String)> {
    if res.levels[..i].iter().any(|s| *s == Said::Waiting) {
        return None;
    }
    let set = res.levels[..i].iter().enumerate().rev().find_map(|(j, s)| Some((s.one()?.to_string(), format!("Inherited from {}", plain(&r.levels[j].fact)))));
    Some(set.map_or_else(|| (r.default.clone(), "Inherited from the default".into()), |(v, from)| (Some(v), from)))
}

/// What sends level L's act for PARTY ("" for a level that holds for everyone) with a value; none when the page does
/// not act or no act of L can be filled.
fn setter<S: 'static>(l: &Level, party: &str, e: &Engine<S>) -> Option<impl Fn(&mut S, &str) + Clone + 'static> {
    let acts = e.acts.as_ref()?;
    let (act, fills) = l.set_by.iter().find_map(|s| Some((s.act.clone(), s.fills.clone()?)))?;
    let (me, send, who) = (acts.me.clone(), acts.send, party.to_string());
    Some(move |s: &mut S, value: &str| {
        let arg = |f: &Fill| Json::from(match f {
            Fill::Actor => me.as_str(),
            Fill::Party => who.as_str(),
            Fill::Value => value,
        });
        send(s, &act, fills.iter().map(arg).collect());
    })
}

/// Row R at level AT: its value there with the act that sets it, or what it inherits; at a party, the engine's own word.
fn row_el<S: 'static>(r: &Row, at: &At, e: &Engine<S>, k: &Kit) -> El {
    let party = if let At::Party(p) = at { p.as_str() } else { "" };
    let res = resolve(e.ask, r, party);
    let place = match at {
        At::Everyone(f) => r.levels.iter().position(|l| l.fact == *f),
        At::Party(_) => r.levels.iter().rposition(|l| l.per_party),
    };
    let value = match place {
        None => {
            let tag = if party.is_empty() { "Not a level of this fact" } else { "Set for everyone" };
            match above(r, &res, r.levels.iter().take_while(|l| !l.per_party).count()) {
                Some((v, _)) => faint_value(k, v.as_deref().unwrap_or("not set"), tag),
                None => said_el(&Said::Waiting, k),
            }
        }
        Some(i) => own(r, &r.levels[i], party, &res.levels[i], above(r, &res, i), e, k),
    };
    let mut right = col().items_end().gap(4.0).child(value);
    if !party.is_empty() && r.per_party {
        right = right.child(now(r, &res, k));
    }
    let acts: Vec<&str> = place.map(|i| r.levels[i].set_by.iter().map(|s| s.act.as_str()).collect()).unwrap_or_default();
    if let Some(x) = (e.refused)(&acts).filter(|_| !acts.is_empty()) {
        let said = format!("{}'s change was refused: {}", x.party, x.why);
        right = right.child(text(said).font(k.ty.small).color(k.c.danger.fg).key(format!("policy-refused-{}", r.fact)));
    }
    let values: Vec<String> = r.values.iter().map(|v| word(v)).collect();
    let fallback: Vec<String> = r.levels.iter().rev().map(from).collect();
    let about = format!("{}; {}{}", values.join(" or "), fallback.join(", else "), r.default.as_ref().map(|d| format!(", else {}", word(d))).unwrap_or_default());
    look::setting(format!("policy-row-{}", r.fact), &word(plain(&r.fact)), &about, right, k)
}

/// Row R's values as a choice NAMED, PICKED drawn picked, each pick sent by SET; with no SET, the picked value alone.
fn choice<S: 'static>(r: &Row, named: &str, picked: Option<usize>, set: Option<impl Fn(&mut S, &str) + Clone + 'static>, k: &Kit) -> El {
    match set {
        Some(set) => {
            let values = r.values.clone();
            let words: Vec<String> = r.values.iter().map(|v| word(v)).collect();
            look::choice(named, &words, picked, move |s: &mut S, j| set(s, &values[j]), k)
        }
        None => text(picked.map_or("not set".to_string(), |i| word(&r.values[i]))).font(k.ty.body).color(k.c.text_base),
    }
}

/// A value as a choice says it: its first letter capital (`guarded`: Guarded), as the Settings page says the modes.
fn word(v: &str) -> String {
    let mut c = v.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// Level L's own value SAID for PARTY: its choice, or what it inherits (ABOVE) muted with Override.
fn own<S: 'static>(r: &Row, l: &Level, party: &str, said: &Said, above: Option<(Option<String>, String)>, e: &Engine<S>, k: &Kit) -> El {
    let set = setter(l, party, e);
    let name = format!("{} {party}", word(plain(&l.fact)));
    match (said, above) {
        (Said::Holds(v), _) if v.len() == 1 => choice(r, name.trim_end(), r.values.iter().position(|x| *x == v[0]), set, k),
        (Said::Holds(v), Some((Some(up), from))) if v.is_empty() => {
            let mut c = faint_value(k, &word(&up), &from);
            if let Some(set) = set {
                let over = Button::new("Override").variant(ButtonVariant::Ghost).on_press(move |s: &mut S| set(s, &up)).build(k);
                c = c.child(over.key(format!("policy-override-{}", r.fact)));
            }
            c
        }
        // nothing to inherit: picked here at once
        (Said::Holds(v), Some((None, _))) if v.is_empty() => choice(r, name.trim_end(), None, set, k),
        (s, _) => said_el(s, k),
    }
}

/// What holds for row R at a party now, and where its settings give it from.
fn now(r: &Row, res: &Resolved, k: &Kit) -> El {
    let whence = res.from.map_or("the default".to_string(), |i| from(&r.levels[i]));
    let given = res.expected(r).map_or("nothing".to_string(), word);
    let (t, warn) = match &res.holds {
        Said::Holds(v) if v.len() == 1 && word(&v[0]) == given => (format!("Now: {given}, {whence}"), false),
        Said::Holds(v) if v.len() == 1 => (format!("Now: {}, though its settings give {given}, {whence}", word(&v[0])), true),
        Said::Holds(v) if v.is_empty() => (format!("Not running now; it would be {given}, {whence}"), false),
        Said::Holds(v) => (format!("Now: {} all at once", v.iter().map(|x| word(x)).collect::<Vec<_>>().join(", ")), true),
        Said::Waiting => ("…".to_string(), false),
        Said::Refused(why) => (format!("Refused: {why}"), true),
    };
    text(t).font(k.ty.small).color(if warn { k.c.warning.fg } else { k.c.text_faint }).key(format!("policy-now-{}", r.fact))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::ask::holds;
    use super::super::plan::{plan, Fill, Plan};
    use super::{page, Acts, At, Engine};
    use rune::runtime::Runtime;
    use rune::{Cx, El};
    use rune_kit::Kit;
    use serde_json::{json, Value as Json};
    use std::path::PathBuf;

    /// The plan of the stdlib and the yaaihh package in module yaaihh.project, as the daemon loads them.
    pub(crate) fn package() -> Plan {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docket");
        let mut texts = Vec::new();
        for dir in ["std", "defaults"] {
            let mut stack = vec![root.join(dir)];
            while let Some(d) = stack.pop() {
                for e in std::fs::read_dir(&d).unwrap_or_else(|e| panic!("{}: {e}", d.display())).map(Result::unwrap) {
                    let p = e.path();
                    if p.is_dir() { stack.push(p) } else if p.extension().is_some_and(|x| x == "docket") { texts.push(p) }
                }
            }
        }
        texts.sort();
        let modules: Vec<_> = texts.iter().map(|p| docket_v2::syntax::read(&std::fs::read_to_string(p).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).collect();
        let loaded = docket_v2::check::load(modules.clone()).expect("the package loads");
        plan(&modules, &loaded, "yaaihh.project")
    }

    /// A page drawn on made-up answers: what a still shows and a test reads.
    pub(crate) struct Still {
        pub plan: Plan,
        pub loaded: Option<String>,
        pub parties: Vec<String>,
        /// The facts that hold: every other question is answered no.
        pub holding: Vec<String>,
        /// The acts the page sent, each with its arguments.
        pub sent: Vec<(String, Vec<Json>)>,
        pub at: Option<At>,
    }

    pub(crate) fn still_init() -> Still {
        let plan = package();
        let parties: Vec<String> = ["w1", "w2", "w3"].map(String::from).to_vec();
        let holding = vec![
            holds("Desk.Project-Mode", &["yolo"]),
            holds("Desk.Agent-Mode", &["w2", "safe"]),
            holds("Desk.Mode-Of", &["w1", "yolo"]),
            holds("Desk.Mode-Of", &["w2", "safe"]),
        ];
        Still { loaded: Some(plan.hash.clone()), plan, parties, holding, sent: Vec::new(), at: None }
    }

    /// The page on the kit's own ground, as the window's pane lays it.
    pub(crate) fn still_view(s: &Still, _: &mut Cx<Still>) -> El {
        let k = Kit::neutral(false);
        let send: fn(&mut Still, &str, Vec<Json>) = |s, act, args| s.sent.push((act.to_string(), args));
        let ask = |q: &str| Some(Ok(s.holding.iter().any(|h| h == q)));
        let acts = Some(Acts { me: "o1".into(), send });
        let e = Engine { loaded: s.loaded.as_deref(), parties: &s.parties, ask: &ask, refused: &|_| None, no: None, acts, at: s.at.as_ref(), go: |s, at| s.at = Some(at) };
        rune::col().w_full().h_full().fill(k.c.bg_base).child(page(&s.plan, &e, &k))
    }

    /// Whether an element under the one keyed KEY has text holding WORDS.
    pub(crate) fn under<S: 'static>(rt: &Runtime<S>, key: &str, words: &str) -> bool {
        rt.elements().into_iter().any(|e| !e.exiting && e.path.split('/').any(|k| k == key) && e.text.as_deref().is_some_and(|t| t.contains(words)))
    }

    /// No text under the element keyed KEY says a level by its fact's name, "The engine", or a mode in lower case.
    pub(crate) fn plain_words<S: 'static>(rt: &Runtime<S>, key: &str) {
        for e in rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == key)) {
            let t = e.text.unwrap_or_default();
            let modes = t.split(|c: char| !c.is_alphanumeric()).any(|w| ["safe", "guarded", "yolo"].contains(&w));
            assert!(!modes && !["Agent-Mode", "Project-Mode", "Install-Mode", "Mode-Of", "The engine"].iter().any(|w| t.contains(w)), "{key}: {t:?}");
        }
    }

    /// Press the element keyed KEY (under the one keyed WITHIN, when given) once it has come in whole, and draw the frame
    /// that runs the press.
    pub(crate) fn press<S: 'static>(rt: &mut Runtime<S>, key: &str, within: Option<&str>) {
        let find = |rt: &Runtime<S>| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key) && within.is_none_or(|w| e.path.split('/').any(|k| k == w)));
        for _ in 0..60 {
            if find(rt).is_some_and(|e| !e.moving && e.opacity >= 0.99 && e.rect == e.target) {
                break;
            }
            rt.frame(0.016);
        }
        let r = find(rt).unwrap_or_else(|| panic!("no {key} under {within:?}")).rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        // one frame runs the press, the next draws what it changed
        rt.frame(0.016);
        rt.frame(0.016);
    }

    /// Pick segment I of the choice in the row keyed ROW.
    pub(crate) fn pick<S: 'static>(rt: &mut Runtime<S>, row: &str, i: usize) {
        press(rt, &format!("segment{i}"), Some(row));
    }

    /// The segment the choice in the row keyed ROW draws picked: the one its indicator moves to.
    pub(crate) fn picked<S: 'static>(rt: &Runtime<S>, row: &str) -> Option<usize> {
        let es: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == row)).collect();
        let ind = es.iter().find(|e| e.key.as_deref() == Some("indicator"))?.target;
        (0..8).find(|i| es.iter().any(|e| e.key.as_deref() == Some(format!("segment{i}").as_str()) && (e.target[0] - ind[0]).abs() < 1.0))
    }

    fn drawn(s: Still) -> Runtime<Still> {
        let mut rt = Runtime::new(s, still_view, (1100.0, 760.0), rune::Env::new(false, 1.0));
        rt.frame(0.0);
        rt
    }

    /// yaaihh's own text gives one row, its mode: the default, then install, project and agent, and nothing else.
    #[test]
    fn yaaihh_s_package_resolves_its_mode_install_then_project_then_agent() {
        let rows = package().rows;
        let names: Vec<&str> = rows.iter().map(|r| r.fact.as_str()).collect();
        assert_eq!(names, ["Desk.Mode-Of"], "the rows the package's text gives");
        let r = &rows[0];
        let levels: Vec<&str> = r.levels.iter().map(|l| l.fact.as_str()).collect();
        assert_eq!(levels, ["Desk.Install-Mode", "Desk.Project-Mode", "Desk.Agent-Mode"]);
        assert_eq!((r.values.clone(), r.default.as_deref()), (vec!["safe".to_string(), "guarded".to_string(), "yolo".to_string()], Some("safe")));
        let agent: Vec<(&str, Option<&[Fill]>)> = r.levels[2].set_by.iter().map(|s| (s.act.as_str(), s.fills.as_deref())).collect();
        assert_eq!(agent, [("yaaihh.desk.Set-Agent-Mode", Some(&[Fill::Actor, Fill::Party, Fill::Value][..]))]);
    }

    /// The levels on the left, the project's shown first: a level's own value is its choice, picked; a level with none
    /// shows what it inherits muted, and where from; a party's level says the engine's own word beside it.
    #[test]
    fn a_level_shows_its_own_value_picked_and_what_it_inherits_muted() {
        let row = "policy-row-Desk.Mode-Of";
        let mut rt = drawn(still_init());
        for level in ["policy-at-Desk.Install-Mode", "policy-at-Desk.Project-Mode", "policy-at-party-w1", "policy-at-party-w3"] {
            assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some(level)), "{level}");
        }
        assert!(under(&rt, row, "Safe or Guarded or Yolo; set for the agent, else from the project, else from this computer, else Safe"));
        assert_eq!(picked(&rt, row), Some(2), "the project's own: yolo");
        for word in ["Safe", "Guarded", "Yolo"] {
            assert!(under(&rt, row, word), "the window's words for the text's modes: {word}");
        }
        plain_words(&rt, row);
        press(&mut rt, "policy-at-Desk.Install-Mode", None);
        assert!(under(&rt, row, "Inherited from the default") && under(&rt, row, "Safe"), "install: nothing set");
        plain_words(&rt, row);
        assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("policy-override-Desk.Mode-Of")), "no act sets it, so no Override");
        press(&mut rt, "policy-at-party-w2", None);
        assert_eq!(picked(&rt, row), Some(0), "w2's own: safe");
        assert!(under(&rt, "policy-now-Desk.Mode-Of", "Now: Safe, set for the agent"));
        plain_words(&rt, row);
        press(&mut rt, "policy-at-party-w1", None);
        assert!(under(&rt, row, "Inherited from the project") && under(&rt, "policy-now-Desk.Mode-Of", "Now: Yolo, from the project"));
        plain_words(&rt, row);
        press(&mut rt, "policy-at-party-w3", None);
        assert!(under(&rt, "policy-now-Desk.Mode-Of", "Not running now; it would be Yolo, from the project"), "w3 runs nowhere");
        plain_words(&rt, row);
    }

    /// A pick sends the level's act, the window's party its actor, then the level's party and the value; Override sends
    /// the inherited value; a pick of the value that holds sends nothing.
    #[test]
    fn a_pick_and_override_send_the_level_s_act() {
        let row = "policy-row-Desk.Mode-Of";
        let mut rt = drawn(still_init());
        pick(&mut rt, row, 0);
        press(&mut rt, "policy-at-party-w3", None);
        press(&mut rt, "policy-override-Desk.Mode-Of", None);
        press(&mut rt, "policy-at-party-w2", None);
        pick(&mut rt, row, 0);
        let mut sent = Vec::new();
        rt.update(|s, _| sent = s.sent.clone());
        let want = [("yaaihh.desk.Set-Project-Mode", json!(["o1", "safe"])), ("yaaihh.desk.Set-Agent-Mode", json!(["o1", "w3", "yolo"]))];
        assert_eq!(sent, want.map(|(a, j)| (a.to_string(), j.as_array().unwrap().clone())), "w2 is safe already");
    }

    /// The page says whether the engine's `loaded` line is the hash of the text it read, and when it has not come.
    #[test]
    fn the_page_says_whether_the_engine_runs_the_text_it_read() {
        let said = |loaded: Option<String>| {
            let rt = drawn(Still { loaded, ..still_init() });
            rt.elements().into_iter().find(|e| e.path.split('/').any(|k| k == "policy-text")).and_then(|e| e.text).unwrap_or_default()
        };
        let hash = package().hash;
        assert!(said(Some(hash.clone())).starts_with("The text the engine runs"), "{}", said(Some(hash.clone())));
        assert!(said(Some("0".repeat(64))).contains("another version"));
        assert!(said(None).contains("not come yet"));
    }

    /// The page as an image, made-up answers on yaaihh's own text, at w1's level: `STILL_OUT=DIR`, one process.
    #[test]
    #[ignore]
    fn policy_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let args = vec!["--snap".to_string(), out.join("policy-v2.png").display().to_string(), "--size".into(), "1100x760".into(), "--at".into(), "1".into()];
        let init = || Still { at: Some(At::Party("w1".into())), ..still_init() };
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1100.0, 760.0), init, still_view) }).args(args).try_run().unwrap();
    }
}
