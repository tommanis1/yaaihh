//! A thread's margin (0.3 W4, proto-lead N-1446 p18 decision 4): the strip down the left of its rows, 28 points wide,
//! the left padding each row had (`threads::rows_in_view`). Each mark standing at its posts (`outline::marks`: its
//! kind says `drawn mark`, proto-lead p17) is drawn there as window-rules §3 has a mark: a dot or a rail, no new
//! element. Which is read from where it stands and when it opened, never from its kind (p44.3, p63.4): a mark that
//! opened after later posts of his is a bracket from the post it stands at to his last post before it opened (a cut
//! tied at #593, opened after his #627: #593–#627); one opened at its own post is a dot (the brain's pass at #619).
//! While it waits on him it is dashed in the accent, the one colour that says he is needed (window-rules); once it has
//! gone it is solid and quiet; dropped or failed, it is not drawn. Its line (`outline::mark_line`) stands among the
//! rows and opens it; the margin says only where it reaches.

use crate::Proj;
use crate::threads::TRow;
use coord_types::TicketView;
use rune::{El, col, row};
use rune_kit::Kit;

/// The margin's width: the rows' left padding it takes the place of.
pub(crate) const WIDTH: f32 = 28.0;
/// One lane of the margin: marks that overlap stand side by side, the first nearest the posts' left edge.
const LANE: f32 = 8.0;
/// Lanes that fit in the margin; a mark past them shares the last.
const LANES: usize = 3;

/// One mark on thread V's margin: its ticket ID, standing at V's post FROM and reaching to his post TO (FROM: a dot),
/// in lane LANE; PROPOSED while it waits on him.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Sign {
    pub id: String,
    pub from: u32,
    pub to: u32,
    pub proposed: bool,
    pub lane: usize,
}

/// Thread V's marks as its margin draws them, in the order of the posts they stand at. A mark with no post to stand
/// at (opened before any) is not drawn here; its line still is.
pub(crate) fn of(p: &Proj, v: &TicketView) -> Vec<Sign> {
    let Some(o) = &v.outline else { return Vec::new() };
    let mut out: Vec<Sign> = crate::outline::marks(p, v)
        .into_iter()
        .filter_map(|t| {
            let c = o.children.iter().find(|c| c.id == t.id)?;
            // a mark that came to nothing is none of `marks` (outline.rs `came_to_nothing`)
            let from = c.at_post?;
            // to his last post before it opened: an agent's post that came in while his posts were held (15 s, N-1445
            // p9) does not stretch a pass taken at his post into a bracket
            let opened = t.opened_at.to_string();
            let to = v.posts.iter().filter(|q| q.n > from && q.by == "owner" && q.at.to_string() < opened).map(|q| q.n).max().unwrap_or(from);
            Some(Sign { id: t.id.clone(), from, to, proposed: c.open && c.owed, lane: 0 })
        })
        .collect();
    out.sort_by_key(|x| (x.from, std::cmp::Reverse(x.to)));
    // each in the first lane no earlier mark still holds at its first post
    let mut ends: Vec<u32> = Vec::new();
    for x in &mut out {
        let free = ends.iter().position(|e| *e < x.from);
        x.lane = match free {
            Some(l) => l,
            None if ends.len() < LANES => {
                ends.push(0);
                ends.len() - 1
            }
            None => LANES - 1,
        };
        ends[x.lane] = ends[x.lane].max(x.to);
    }
    out
}

/// Where a row of a thread's page stands among its posts: at post N, after post N (a line, a card or a struck post
/// between N and the next), or before the first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum At {
    Before,
    Post(u32),
    After(u32),
}

/// Where each of ROWS stands (`threads::seq`'s order).
pub(crate) fn places(rows: &[TRow]) -> Vec<At> {
    let mut last = None;
    rows.iter()
        .map(|r| match r {
            TRow::Post(n) => {
                last = Some(*n);
                At::Post(*n)
            }
            _ => last.map_or(At::Before, At::After),
        })
        .collect()
}

/// Whether X reaches the row at AT: its posts, and what stands between them (a dot only its post).
pub(crate) fn covers(x: &Sign, at: At) -> bool {
    match at {
        At::Post(n) => x.from <= n && n <= x.to,
        At::After(n) => x.from <= n && n < x.to,
        At::Before => false,
    }
}

/// The margin beside the row at AT: a lane for each of SIGNS, drawn where it reaches the row.
pub(crate) fn cell(signs: &[Sign], at: At, k: &Kit) -> El {
    let lanes = (0..LANES).map(|l| match signs.iter().find(|x| x.lane == l && covers(x, at)) {
        Some(x) => lane(x, at, k),
        None => col().w(LANE),
    });
    row().w(WIDTH).self_stretch().pl(2.0).children(lanes)
}

/// Mark X's part in the row at AT: a dot at its post; else its rail, with a tick where it starts and where it ends.
fn lane(x: &Sign, at: At, k: &Kit) -> El {
    let c = if x.proposed { k.c.text_accent } else { k.c.text_muted };
    let key = match at {
        At::Post(n) => Some(format!("margin-{}-p{n}", x.id)),
        _ => None,
    };
    let el = if x.from == x.to {
        let dot = col().mt(7.0).w(7.0).h(7.0).radius(3.5);
        let dot = if x.proposed { dot.border(1.5, c).border_dash(2.0, 2.0) } else { dot.fill(c) };
        col().w(LANE).self_stretch().child(dot)
    } else {
        let line = col().w(2.0).grow(1.0);
        let line = if x.proposed { line.border(1.0, c).border_dash(3.0, 3.0) } else { line.fill(c) };
        let tick = || row().w(LANE - 2.0).h(2.0).fill(c);
        col().w(LANE).self_stretch().items_start().children((at == At::Post(x.from)).then(tick)).child(line).children((at == At::Post(x.to)).then(tick))
    };
    match key {
        Some(key) => el.key(key),
        None => el,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// N-1104 with his posts and agents' between, and marks standing at its posts: (id, at_post, opened_at, state,
    /// owed, open).
    fn thread(marks: &[(&str, u32, &str, &str, bool, bool)]) -> Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        let posts = json!([
            {"n": 593, "by": "owner", "at": "2026-11-20T18:25:49Z", "text": "i think we are splitting it up too small"},
            {"n": 594, "by": "paper-research", "at": "2026-11-20T18:26:30Z", "text": "One structure."},
            {"n": 619, "by": "owner", "at": "2026-11-20T19:01:07Z", "text": "brain, exact mechanism"},
            {"n": 620, "by": "paper-research", "at": "2026-11-20T19:01:15Z", "text": "Build 35 is in."},
            {"n": 627, "by": "owner", "at": "2026-11-20T19:05:00Z", "text": "and the cut"},
            {"n": 628, "by": "paper-research", "at": "2026-11-20T19:05:40Z", "text": "Yes."},
            {"n": 629, "by": "owner", "at": "2026-11-20T19:07:14Z", "text": "later"}]);
        let children: Vec<_> = marks.iter().map(|(id, at, opened, state, owed, open)| json!({"id": id, "label": id, "parent": "N-1104", "at_post": at, "state": state, "owed": owed, "open": open, "opened_at": opened})).collect();
        let v: TicketView = serde_json::from_value(json!({"id": "N-1104", "n": 1104, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "Your one group chat", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "drawn": "line",
            "opened_at": "2026-11-16T07:00:00Z", "given": [], "posts": posts,
            "outline": {"id": "N-1104", "label": "Your one group chat", "state": "quiet", "open": true, "opened_at": "2026-11-16T07:00:00Z", "children": children}})).unwrap();
        p.live.tickets.insert(1104, v);
        for (i, (id, _, opened, ..)) in marks.iter().enumerate() {
            let m: TicketView = serde_json::from_value(json!({"id": id, "n": i as u64 + 1, "kind": "aside", "opener": "owner", "ctx": {}, "form": {}, "title": id,
                "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "drawn": "mark", "opened_at": opened, "given": [], "posts": []})).unwrap();
            p.live.tickets.insert(m.n, m);
        }
        p
    }

    /// S on N-1104's page with two passes of paper-research's brain on it, each opened by its rule: B-1, done, spoke at
    /// #619; B-2 at #629, at work, spoke too when SPOKE.
    fn brain_passes(s: &mut crate::App, spoke: bool) {
        s.notice = None;
        let mut p = thread(&[("B-1", 619, "2026-11-20T19:01:22Z", "done", false, false), ("B-2", 629, "2026-11-20T19:07:30Z", "at work", false, true)]);
        p.down = None;
        for n in [1u64, 2] {
            let t = p.live.tickets.get_mut(&n).unwrap();
            // brain.kind's `drawn note` (proto-mind 175c1dd2): a mark drawn once it has spoken
            (t.kind, t.title, t.drawn) = ("brain".into(), "one pass of an agent's brain".into(), Some(docket_machine::syntax::Drawn::Note));
            t.binding = Some(RULE.into());
        }
        p.live.tickets.get_mut(&1).unwrap().posts =
            serde_json::from_value(json!([{"n": 1, "by": "B-1", "at": "2026-11-20T19:01:40Z", "text": "He asks for the exact mechanism: I answer with the brain's steps before the build."}])).unwrap();
        if spoke {
            p.live.tickets.get_mut(&2).unwrap().posts =
                serde_json::from_value(json!([{"n": 1, "by": "B-2", "at": "2026-11-20T19:07:50Z", "text": "He says later: I leave the cut and finish the answer he is waiting on."}])).unwrap();
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(1104);
    }

    /// The rule that opens a brain pass in [`brain_passes`].
    const RULE: &str = "on owner_to(m) where m.agent == \"paper-research\" open brain(m.agent, Attend(m.posts))";

    /// A mark a rule made ends in a small ⓘ (the owner, N-1790 part 3): pressed, what is behind it opens apart, and not
    /// its thread; the rule and what went in and came out are in it, and its Raw Wire and Docket picture each leave the
    /// popup for them. A mark no rule made has none.
    #[test]
    fn a_rule_made_marks_info_opens_what_is_behind_it_and_its_jumps() {
        let settle = |rt: &mut rune::runtime::Runtime<crate::App>| {
            rt.invalidate();
            for _ in 0..4 {
                rt.frame(0.016);
            }
        };
        let open = |rt: &mut rune::runtime::Runtime<crate::App>| {
            crate::view_cost::click(rt, "behind-press-B-1");
            settle(rt);
        };
        {
            let (_held, mut s) = crate::view_cost::test_app();
            brain_passes(&mut s, true);
            let mut rt = crate::view_cost::window(s);
            settle(&mut rt);
            assert!(rt.find("behind-press-B-1").is_some() && rt.find("behind-press-B-2").is_some(), "each pass ends in its ⓘ");
            open(&mut rt);
            assert_eq!(rt.state.popups.last().map(|p| &p.what), Some(&crate::popup::What::Behind { n: 1, on: 1104 }), "what is behind it, apart");
            assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&crate::Sel::Tickets, Some(1104)), "the press is the ⓘ's: the thread stays");
            let under = |rt: &rune::runtime::Runtime<crate::App>, w: &str| crate::popup::tests::text_under(rt, "behind-body-B-1", w).is_some();
            for w in ["Verdict", "done", RULE, "In → out", "nothing beyond what its rule names", "1 post", "1 of 1 steps", "Raw Wire ›", "Docket picture ›"] {
                assert!(under(&rt, w), "{w:?} behind it");
            }
            // its Raw Wire: the popup closes and the lens stands on its calls
            crate::view_cost::click(&mut rt, "behind-wire-B-1");
            settle(&mut rt);
            assert!(rt.state.popups.is_empty(), "the popup leaves for its wire");
            assert!(matches!(&rt.state.sel, crate::Sel::Lens(l) if l.depth == crate::lens::Depth::Wire && l.at.as_deref() == Some("B-1")), "{:?}", rt.state.sel);
            // its Docket picture
            rt.update(|s, _| {
                s.sel = crate::Sel::Tickets;
                s.tickets.shown = Some(1104);
            });
            settle(&mut rt);
            open(&mut rt);
            crate::view_cost::click(&mut rt, "behind-docket-B-1");
            settle(&mut rt);
            assert!(rt.state.popups.is_empty(), "the popup leaves for its picture");
            assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&crate::Sel::Kinds, Some(1)), "its own program, in the Docket");
        }
        // a mark no rule made: no ⓘ
        let (_held, mut s) = crate::view_cost::test_app();
        s.notice = None;
        let mut p = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "waits on you", true, true)]);
        p.down = None;
        s.projects = vec![p];
        (s.cur, s.sel, s.tickets.shown) = (0, crate::Sel::Tickets, Some(1104));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(rt.find("mark-A-1").is_some() && rt.find("behind-press-A-1").is_none(), "his own aside: nothing behind it but him");
    }

    fn signs(p: &Proj) -> Vec<Sign> {
        of(p, &p.live.tickets[&1104])
    }

    /// A mark that opened after later posts of his is a bracket to his last post before it opened, dashed while it
    /// waits on him and solid once it has gone (p44.3; S3's cut at #593–#627).
    #[test]
    fn a_mark_opened_after_his_later_posts_is_a_bracket_to_the_last_of_them() {
        let waits = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "waits on you", true, true)]);
        assert_eq!(signs(&waits), [Sign { id: "A-1".into(), from: 593, to: 627, proposed: true, lane: 0 }]);
        let gone = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "at work", false, true)]);
        assert_eq!(signs(&gone), [Sign { id: "A-1".into(), from: 593, to: 627, proposed: false, lane: 0 }]);
        let done = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "done", false, false)]);
        assert!(!signs(&done)[0].proposed);
    }

    /// A pass taken at his post is a dot there, even when an agent posted while his post was held (his 15 s, N-1445
    /// p9): only his own posts stretch a mark.
    #[test]
    fn a_mark_at_his_post_is_a_dot_though_an_agent_posted_before_it_opened() {
        let p = thread(&[("B-1", 619, "2026-11-20T19:01:22Z", "done", false, false)]);
        assert_eq!(signs(&p), [Sign { id: "B-1".into(), from: 619, to: 619, proposed: false, lane: 0 }]);
    }

    /// He said no (the kind's no ends `Dropped`), it failed, or a brain's pass said nothing (proto-mind N-1446 p270):
    /// nothing on the margin. A thread with no marks has no margin at all, so its rows are drawn as before.
    #[test]
    fn a_dropped_or_failed_mark_and_a_thread_with_none_draw_nothing() {
        assert!(signs(&thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "dropped", false, false)])).is_empty());
        assert!(signs(&thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "failed", false, false)])).is_empty());
        assert!(signs(&thread(&[("B-1", 619, "2026-11-20T19:01:22Z", "said nothing", false, false)])).is_empty());
        assert!(signs(&thread(&[])).is_empty());
    }

    /// Marks that overlap stand in lanes side by side; one that starts after another ends takes its lane again.
    #[test]
    fn overlapping_marks_stand_side_by_side() {
        let p = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "done", false, false), ("B-1", 619, "2026-11-20T19:01:22Z", "done", false, false), ("B-2", 629, "2026-11-20T19:07:30Z", "done", false, false)]);
        let lanes: Vec<(String, usize)> = signs(&p).into_iter().map(|x| (x.id, x.lane)).collect();
        assert_eq!(lanes, [("A-1".to_string(), 0), ("B-1".to_string(), 1), ("B-2".to_string(), 0)]);
    }

    /// A bracket reaches its posts and the rows between them (its own line after #593 among them), a dot only its
    /// post, and nothing before the first post or after the last.
    #[test]
    fn a_bracket_reaches_the_rows_between_its_posts() {
        let p = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "done", false, false), ("B-1", 619, "2026-11-20T19:01:22Z", "done", false, false)]);
        let v = &p.live.tickets[&1104];
        let rows = crate::threads::seq(&p, v);
        let at = places(&rows);
        let s = signs(&p);
        let (a, b) = (&s[0], &s[1]);
        let reach = |x: &Sign| rows.iter().zip(&at).filter(|(_, at)| covers(x, **at)).map(|(r, _)| r.clone()).collect::<Vec<_>>();
        assert_eq!(reach(a), [TRow::Post(593), TRow::Opened(1), TRow::Post(594), TRow::Post(619), TRow::Opened(2), TRow::Post(620), TRow::Post(627)], "{rows:?}");
        assert_eq!(reach(b), [TRow::Post(619)]);
        assert_eq!(at.first(), Some(&At::Before), "the header stands before the first post");
    }

    /// Drawn: the bracket's lane beside each of its posts, the dot beside its post; nothing keyed beside a post it
    /// does not reach.
    #[test]
    fn the_margin_is_drawn_beside_the_rows_it_reaches() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", "waits on you", true, true), ("B-1", 619, "2026-11-20T19:01:22Z", "done", false, false)]);
        p.down = None;
        s.projects = vec![p];
        s.cur = 0;
        let mut rt = crate::view_cost::window(s);
        rt.update(|s, _| {
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(1104);
        });
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert!(rt.find("post-N-1104-629").is_some(), "the thread is drawn to its last post");
        for n in [593, 594, 619, 620, 627] {
            assert!(rt.find(&format!("margin-A-1-p{n}")).is_some(), "the bracket beside #{n}");
        }
        assert!(rt.find("margin-A-1-p628").is_none() && rt.find("margin-A-1-p629").is_none(), "not past his #627");
        assert!(rt.find("margin-B-1-p619").is_some() && rt.find("margin-B-1-p620").is_none(), "the dot at #619 only");
    }

    /// The margin's stills, on a made-up N-1104 (nothing of his record), in two states of one cut tied at #593 that
    /// opened after his #627, beside a brain's pass taken at his #619 that has gone (a quiet dot, its line under #619):
    /// - margin.png: the cut waits for his press, as it does while the policy has no `lapse-after` for it
    ///   (first-show-asks item 2): a dashed bracket in the accent over #593-#627, its line under #593;
    /// - margin-lapses.png: the same cut once the policy gives it a time: coordd's `lapses_at` (19:07Z, 20:07 on his
    ///   clock) makes its outline node not owed, its state "goes through at 20:07" (E1b, proto-lead N-1446 p136, p147),
    ///   so the bracket is quiet. The words in its title stand in for the held mark's, which are the next head's.
    ///
    /// And a brain's pass taken at his #629, opened by research's binding, its kind `drawn note` (proto-mind N-1446 p906,
    /// 175c1dd2; `outline::not_yet`):
    /// - pass-at-work.png: its calls running, nothing said yet: no line under #629 and no dot beside it;
    /// - pass-spoke.png: its thought posted: "◆ one pass of an agent's brain · {the thought}" under #629 (the kind's
    ///   label up to its ':', `outline::mark_kind`), its dot beside it.
    ///
    /// The stills of the ⓘ (the owner, N-1790 part 3; STILL_OUT, made-up passes): N-1104 with its two passes, each
    /// ending in its ⓘ (behind-marks.png), and B-1's open (behind-open.png).
    /// `STILL_OUT=DIR yaaihh-tests margin::tests::behind_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn behind_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn marks() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            brain_passes(&mut s, true);
            s
        }
        fn opened() -> crate::App {
            let mut s = marks();
            s.popups.push(crate::popup::Popup { title: crate::popup::title("one pass of an agent's brain", "B-1", "behind it"), what: crate::popup::What::Behind { n: 1, on: 1104 } });
            s
        }
        for (name, app) in [("behind-marks", marks as fn() -> crate::App), ("behind-open", opened)] {
            let script = out.join(format!("{name}.json"));
            std::fs::write(&script, "[]").unwrap();
            let snap = out.join(format!("{name}.png")).display().to_string();
            let script = script.display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", "1400x900", "--at", "1", "--script", &script];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), app, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }

    /// `STILL_OUT=DIR yaaihh-tests margin::tests::margin_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn margin_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn scene(lapses: bool) -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            let state = if lapses { "goes through at 20:07" } else { "waits on you" };
            let mut p = thread(&[("A-1", 593, "2026-11-20T19:05:20Z", state, !lapses, true), ("B-1", 619, "2026-11-20T19:01:22Z", "done", false, false)]);
            p.down = None;
            let cut = if lapses { "Brain fundamentals · goes through at 20:07 · Not this" } else { "Brain fundamentals · Not this" };
            for (n, kind, title) in [(1u64, "aside", cut), (2, "brain", "paper-research is off your topic (builds) → answer p619 concretely")] {
                let t = p.live.tickets.get_mut(&n).unwrap();
                (t.kind, t.title) = (kind.into(), title.into());
            }
            if lapses {
                p.live.tickets.get_mut(&1).unwrap().lapses_at = Some("2026-11-20T19:07:00Z".into());
            }
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(1104);
            s
        }
        fn waits() -> crate::App {
            scene(false)
        }
        fn lapses() -> crate::App {
            scene(true)
        }
        fn pass(spoke: bool) -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            brain_passes(&mut s, spoke);
            s
        }
        fn at_work() -> crate::App {
            pass(false)
        }
        fn spoke() -> crate::App {
            pass(true)
        }
        for (name, app) in [("margin", waits as fn() -> crate::App), ("margin-lapses", lapses), ("pass-at-work", at_work), ("pass-spoke", spoke)] {
            let script = out.join(format!("{name}.json"));
            std::fs::write(&script, "[]").unwrap();
            let snap = out.join(format!("{name}.png")).display().to_string();
            let script = script.display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", "1400x900", "--at", "1", "--script", &script];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), app, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }
}
