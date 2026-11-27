//! The picture's wires (0.2 line 26, docket-edge8 N-1396 p132): drawn on rune's canvas (`canvas(Shape)`), in place
//! of glyphs. A wire runs down a block's spine, a fixed place from its left edge, so a statement's arrow, a fork's
//! drops and a lane's all line up; its head is rune's `shape::arrowhead`. Dashed (rune's `Shape::dashed`): what is to
//! come, and a drop into an arm not taken. A bus across a fork's arms or a par's lanes is one piece per arm, each its
//! arm's own width, so it spans whatever width the row lays them out at.

use rune::shape::arrowhead;
use rune::{Color, El, LineCap, Shape, canvas, col, el, row};

/// Where a block's wire runs, from the block's left edge.
pub(crate) const SPINE: f32 = 18.0;
/// A wire's width.
pub(crate) const WIDTH: f32 = 1.5;
/// A head: from its tip back to its base, and across its base.
pub(crate) const HEAD: [f32; 2] = [head_len(WIDTH), head_len(WIDTH) * 0.9];
/// A head's length on a wire WIDTH wide: seven points and 1.6 widths, so a thicker wire (the Docket's, as thick as what
/// waits there) carries a larger head; its base is nine tenths of that across.
pub(crate) const fn head_len(width: f32) -> f32 {
    7.0 + width * 1.6
}
/// A dashed wire's dash and gap.
pub(crate) const DASH: [f32; 2] = [4.0, 3.0];
/// A wire's element: the spine and a little past it, so what stands on the wire (a value, a case) goes just beside it.
const BESIDE: f32 = SPINE + 8.0;
/// A ◇'s or a bar's element: room for its shape round the spine.
const ROUND: f32 = SPINE * 2.0;

/// S dashed when DASHED; its fills (a head) stay whole.
pub(crate) fn dashed(s: Shape, dashed: bool) -> Shape {
    if dashed { s.dashed(DASH[0], DASH[1]) } else { s }
}

/// S with the head of a wire WIDTH wide at TIP, pointing along DIR (a unit vector), in INK.
pub(crate) fn head(s: Shape, tip: [f32; 2], dir: [f32; 2], width: f32, ink: Color) -> Shape {
    let l = head_len(width);
    s.fill(arrowhead(tip, dir, l, l * 0.9), ink)
}

/// A wire H points down the spine into what is under it, its head at the foot. The stem stops at the head's base, so
/// a translucent ink is not laid twice.
pub(crate) fn down(ink: Color, dash: bool, h: f32) -> El {
    let stem = Shape::new().line([SPINE, 0.0], [SPINE, (h - HEAD[0]).max(0.0)], WIDTH, LineCap::Butt, ink);
    canvas(head(dashed(stem, dash), [SPINE, h], [0.0, 1.0], WIDTH, ink)).w(BESIDE).min_w(BESIDE).h(h)
}

/// A wire H points down the spine with no head: on into a bus, or out of a lane into the join.
pub(crate) fn stem(ink: Color, dash: bool, h: f32) -> El {
    canvas(dashed(Shape::new().line([SPINE, 0.0], [SPINE, h], WIDTH, LineCap::Butt, ink), dash)).w(BESIDE).min_w(BESIDE).h(h)
}

/// Arm I of N's piece of the bus across them, its own width, at its top (a split) or its foot (a join): from its spine
/// to its right edge (the first), across it (one between), from its left edge to its spine (the last). LEFT inks the
/// part up to its spine (the way into this arm), RIGHT the part after it (the way on to the arms after).
pub(crate) fn bus(i: usize, n: usize, left: Color, right: Color) -> El {
    let half = WIDTH / 2.0;
    // the first from its own spine's left side, so the corner with the wire above is whole
    let from = if i == 0 { SPINE - half } else { 0.0 };
    let mut r = row().w_full().h(WIDTH).child(el().w(from).min_w(from).h(WIDTH)).child(el().w(SPINE + half - from).min_w(SPINE + half - from).h(WIDTH).fill(left));
    if i + 1 < n {
        r = r.child(el().grow(1.0).w(0.0).h(WIDTH).fill(right));
    }
    r
}

/// A wire down the spine that grows with what stands beside it (a header of more than one line), at least H points.
fn on_down(on: Color, h: f32) -> El {
    el().w(WIDTH).min_w(WIDTH).ml(SPINE - WIDTH / 2.0).grow(1.0).min_h(h).fill(on)
}

/// A match's ◇ on the spine, its outline in INK over FILL, and a wire under it in ON, on to the bus: at least H points,
/// as tall as the match's words beside it.
pub(crate) fn diamond(ink: Color, fill: Color, on: Color, h: f32) -> El {
    let r = 7.0;
    let mid = r + 1.0;
    let corners = vec![[SPINE, mid - r], [SPINE + r, mid], [SPINE, mid + r], [SPINE - r, mid]];
    let s = Shape::new().fill(corners.clone(), fill).stroke_with(corners, true, WIDTH, LineCap::Round, ink);
    col().w(ROUND).min_w(ROUND).child(canvas(s).w(ROUND).h(mid + r + WIDTH / 2.0)).child(on_down(on, h))
}

/// A par's split: a bar across the spine (as a flow chart draws a fork), and a wire under it on to the bus, at least H
/// points, all in INK.
pub(crate) fn bar(ink: Color, h: f32) -> El {
    let s = Shape::new().line([SPINE - 9.0, 2.0], [SPINE + 9.0, 2.0], 3.0, LineCap::Round, ink);
    col().w(ROUND).min_w(ROUND).child(canvas(s).w(ROUND).h(3.5)).child(on_down(ink, h))
}

/// An iterate's arrow back, in INK: up the frame's left side from its foot and into its top, the head pointing in.
/// The side grows with the frame; the corners are drawn, so it is one line at any height.
pub(crate) fn back(ink: Color) -> El {
    const W: f32 = 14.0;
    let x = 3.0;
    let (y, tip) = (6.0, [W, 6.0]);
    let top = Shape::new().stroke_with(vec![[x, 12.0], [x, y], [W - HEAD[0] + 1.0, y]], false, WIDTH, LineCap::Butt, ink);
    let top = canvas(top.fill(arrowhead(tip, [1.0, 0.0], HEAD[0] - 1.0, HEAD[1] - 1.0), ink)).w(W).min_w(W).h(12.0);
    let side = el().w(WIDTH).ml(x - WIDTH / 2.0).grow(1.0).fill(ink);
    let foot = canvas(Shape::new().stroke_with(vec![[x, 0.0], [x, 6.0], [W, 6.0]], false, WIDTH, LineCap::Butt, ink)).w(W).min_w(W).h(8.0);
    col().w(W).min_w(W).mb(10.0).child(top).child(side).child(foot)
}

/// The wires drawn in the window's own picture (picture.rs `Pic`) of made-up kinds, plain (the Kinds page) and lit by a
/// made-up ticket's run (its page): where each sits, and what a press on a fold does.
#[cfg(test)]
mod tests {
    use crate::App;
    use crate::picture::Pic;
    use coord_types::TicketView;
    use serde_json::{Value, json};
    use std::cell::RefCell;

    type Rt = rune::runtime::Runtime<App>;

    /// agent-base's question kind, as written there today.
    const QUESTION: &str = "kind question
  form ask: Text, context: Text?
  result Answered(text: Text) | Dismissed(why: Text)
  pattern
    a <- step role lead answer(ask, context);
    match a {
      Answer(text) => return Answered(text)
      Dismiss(why) => return Dismissed(why)
      Up(why) =>
        o <- step owner reply(ask, context, why);
        match o {
          Answer(text) => return Answered(text)
          Dismiss(why) => return Dismissed(why)
        }
    }
";

    /// Two steps side by side, then one after them.
    const BOTH: &str = "kind both
  form what: Text
  result Ref
  pattern
    (x, y) <- par(step role lead one(what), step role lead two(what, more, words, so, it, runs, longer));
    return Done(x)
";

    /// A retry round a step and a match.
    const PIECE: &str = "kind piece
  form task: Text, who: Agent
  result Ref
  raises NoPart
  pattern
    iterate errs = \"none\" up to 3 {
      r <- step who \"Implement this: {{ task }}\" -> Commit(c: Ref) | Cannot(why: Text);
      match r {
        Cannot(why) => raise NoPart(why),
        Commit(c) => (v <- step role runner check(c, none);
          match v { Green => return Done(c), Red(log) => return Again(log) })
      }
    }
";

    thread_local! {
        static SHOWN: RefCell<(&'static str, Option<TicketView>)> = const { RefCell::new(("", None)) };
    }

    /// The kind SHOWN holds, drawn as the Kinds page draws it (its picture under key `q`), lit by its ticket if any.
    fn view(s: &App, cx: &mut rune::Cx<App>) -> rune::El {
        let k = rune_kit::Kit::neutral(cx.system_dark());
        let words = |v: &Value| v.as_str().map(String::from).unwrap_or_else(|| v.to_string());
        SHOWN.with(|x| {
            let x = x.borrow();
            let pic = Pic { k: &k, p: None, kinds: Vec::new(), tickets: Vec::new(), open: &s.tickets.pic_open, marks: crate::edit::marks(&s.tickets), words: &words, pick: None, picked: None };
            rune::col().key("alone").w(900.0).padding(20.0).child(pic.kind(x.0, x.1.as_ref(), "q"))
        })
    }

    fn shown(text: &'static str, v: Option<TicketView>) -> (std::sync::MutexGuard<'static, ()>, Rt) {
        SHOWN.with(|x| *x.borrow_mut() = (text, v));
        let (held, s) = crate::view_cost::test_app();
        let mut rt = rune::runtime::Runtime::new(s, view, (1000.0, 1600.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        (held, rt)
    }

    fn settle(rt: &mut Rt) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    /// KEY's place, unless it is gone (an element playing its exit stays listed, `exiting`).
    fn rect(rt: &Rt, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).map(|e| e.rect)
    }

    /// The drops into the arms of the match at OF, left to right.
    fn drops(rt: &Rt, of: &str) -> Vec<[f32; 4]> {
        (0..).map_while(|i| rect(rt, &format!("pic-{of}.a{i}-drop"))).collect()
    }

    fn texts(rt: &Rt) -> Vec<String> {
        rt.elements().into_iter().filter_map(|e| e.text).collect()
    }

    fn press(rt: &mut Rt, key: &str) {
        let r = rect(rt, key).unwrap_or_else(|| panic!("no {key}"));
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn node(what: &str, state: &str, value: Option<Value>, nodes: Vec<Value>) -> Value {
        json!({"what": what, "line": 0, "source": 0, "text": "", "state": state, "value": value, "nodes": nodes})
    }

    /// A question its lead passed up: at the owner's reply now, the other two arms not taken.
    fn passed_up() -> TicketView {
        let run = vec![
            node("step", "done", Some(json!({"head": "Up", "why": "which of the two"})), vec![]),
            node(
                "match",
                "now",
                None,
                vec![
                    node("arm", "not_taken", None, vec![]),
                    node("arm", "not_taken", None, vec![]),
                    node("arm", "now", None, vec![node("step", "now", None, vec![]), node("match", "to_come", None, vec![node("arm", "to_come", None, vec![]), node("arm", "to_come", None, vec![])])]),
                ],
            ),
        ];
        serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "question", "opener": "kit-3", "ctx": {}, "form": {"ask": "which branch?"},
            "title": "which branch?", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-20T12:00:00Z", "given": [], "posts": [], "run": run}))
        .unwrap()
    }

    /// The Kinds page's question: a ◇, then its three arms side by side, a drop into each from one bus, the first on
    /// the spine the step above it stands on; the arm holding a step and a match is the widest; no glyph is a wire.
    #[test]
    fn a_match_is_a_diamond_and_a_drop_into_each_arm_side_by_side() {
        let (_held, rt) = shown(QUESTION, None);
        let d = drops(&rt, "q.1");
        assert_eq!(d.len(), 3, "a drop into each arm: {d:?}");
        assert!(d.windows(2).all(|w| w[0][1] == w[1][1] && w[0][0] < w[1][0]), "side by side, left to right: {d:?}");
        let step = rect(&rt, "pic-q.0").expect("the lead's step");
        let fork = rect(&rt, "pic-q.1").expect("the match");
        assert_eq!(d[0][0], step[0], "the first drop on the step's spine");
        assert!(d[0][1] > fork[1] + 14.0, "under the ◇ and its words");
        let wide = |i: usize| if i + 1 < d.len() { d[i + 1][0] - d[i][0] } else { fork[0] + fork[2] - d[i][0] };
        assert!(wide(2) > wide(0) && wide(2) > wide(1), "the arm with a step and a match in it is the widest: {:?}", (wide(0), wide(1), wide(2)));
        let inner = drops(&rt, "q.1.a2.1");
        assert_eq!(inner.len(), 2, "the owner's match: two drops");
        assert_eq!(inner[0][0], d[2][0], "on the spine of the arm it is in");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Up(why)") && t.iter().any(|x| x == "Dismiss(why)"), "each case on its drop: {t:?}");
        assert!(!t.iter().any(|x| ["▼", "◇", "⫴", "◀"].iter().any(|g| x.contains(g))), "no glyph stands for a wire: {t:?}");
        assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.ends_with("-fold"))), "the definition folds nothing");
    }

    /// Lit by a question passed up: the two arms not taken are one "not taken" each and their bodies are not drawn,
    /// the arm taken has the room; a press opens one, drawn as it was, and a second press folds it again.
    #[test]
    fn lit_an_arm_not_taken_folds_to_one_pill_a_press_opens_and_folds_again() {
        let (_held, mut rt) = shown(QUESTION, Some(passed_up()));
        let d = drops(&rt, "q.1");
        assert_eq!(d.len(), 3);
        assert!(rect(&rt, "pic-q.1.a0-fold").is_some() && rect(&rt, "pic-q.1.a1-fold").is_some(), "the arms not taken fold");
        assert!(rect(&rt, "pic-q.1.a2-fold").is_none(), "the arm taken does not");
        assert!(rect(&rt, "pic-q.1.a0.0").is_none() && rect(&rt, "pic-q.1.a1.0").is_none(), "their bodies are not drawn");
        assert!(rect(&rt, "pic-q.1.a2.0").is_some(), "the owner's reply is");
        assert_eq!(texts(&rt).iter().filter(|x| *x == "not taken ›").count(), 2);
        let fork = rect(&rt, "pic-q.1").unwrap();
        assert!(fork[0] + fork[2] - d[2][0] > 2.0 * (d[1][0] - d[0][0]), "the arm taken has the room: {d:?} in {fork:?}");
        press(&mut rt, "pic-q.1.a0-fold");
        assert!(rt.state.tickets.pic_open.contains("q.1.a0.shown"), "{:?}", rt.state.tickets.pic_open);
        assert!(rect(&rt, "pic-q.1.a0.0").is_some(), "opened: its return is drawn");
        assert!(rect(&rt, "pic-q.1.a1.0").is_none(), "the other stays folded");
        assert!(texts(&rt).iter().any(|x| x == "not taken ⌄"), "its pill folds it again");
        press(&mut rt, "pic-q.1.a0-fold");
        assert!(!rt.state.tickets.pic_open.contains("q.1.a0.shown"), "{:?}", rt.state.tickets.pic_open);
        assert!(rect(&rt, "pic-q.1.a0.0").is_none(), "folded again");
    }

    /// A par: a drop into each lane at one height from the bus under its bar, the first on the spine; the lanes as
    /// tall as the tallest, so their ways out to the join start at one height; the step after it on the same spine.
    #[test]
    fn a_par_drops_into_each_lane_and_joins_under_them() {
        let (_held, rt) = shown(BOTH, None);
        let (a, b) = (rect(&rt, "pic-q.0.b0-drop").expect("into lane 1"), rect(&rt, "pic-q.0.b1-drop").expect("into lane 2"));
        assert!(a[1] == b[1] && a[0] < b[0], "side by side: {a:?} {b:?}");
        let par = rect(&rt, "pic-q.0").unwrap();
        assert_eq!(a[0], par[0], "the first on the spine");
        let (l1, l2) = (rect(&rt, "pic-q.0.b0").unwrap(), rect(&rt, "pic-q.0.b1").unwrap());
        assert_eq!(l1[1] + l1[3], l2[1] + l2[3], "the lanes end at one height");
        let after = rect(&rt, "pic-q.1").expect("the return after it");
        assert!(after[1] > l1[1] + l1[3] + 10.0, "under the join");
        assert_eq!(after[0], par[0]);
    }

    /// A retry round a step and a match: its arrow back is drawn, not written.
    #[test]
    fn an_iterates_arrow_back_is_a_wire() {
        let (_held, rt) = shown(PIECE, None);
        assert_eq!(drops(&rt, "q.0.i.1").len(), 2, "the match inside the round: {:?}", texts(&rt));
        let t = texts(&rt);
        assert!(!t.iter().any(|x| x.contains('◀') || x.contains('▼')), "{t:?}");
        assert!(t.iter().any(|x| x.starts_with("↺ Again")), "its foot still says it: {t:?}");
    }

    /// The question kind as coordd's Kinds answer gives it.
    fn question_form() -> crate::kinds::KindForm {
        serde_json::from_value(json!({"kind": "question", "letter": "Q", "form": [["ask", "Text"], ["context", {"Opt": "Text"}]],
            "result": {"Variant": [["Answered", [["text", "Text"]]], ["Dismissed", [["why", "Text"]]]]}, "raises": [], "bound": 2, "text": QUESTION}))
        .unwrap()
    }

    /// The par kind, BOTH, as the Kinds answer would give it.
    fn both_form() -> crate::kinds::KindForm {
        serde_json::from_value(json!({"kind": "both", "letter": "B", "form": [["what", "Text"]], "result": "Ref", "raises": [], "bound": 2, "text": BOTH})).unwrap()
    }

    /// agent-base's delivery.kind, the message runtime's program, as docket.rs's tests give it (docket-edge8, p172).
    fn delivery_form() -> crate::kinds::KindForm {
        serde_json::from_value(json!({"kind": "delivery", "form": [], "text": super::super::docket::tests::delivery_text(), "doc": "", "bindings": []})).unwrap()
    }

    /// A stand-in project with the question kind, the par kind and delivery, and the passed-up question as its ticket 7; the lock
    /// of the tests that make a whole App held while it lives.
    fn stand_in() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "Research (stand-in)".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let mut v = passed_up();
        v.program = QUESTION.to_string();
        p.live.tickets.insert(v.n, v);
        s.projects = vec![p];
        s.cur = 0;
        s.tickets.kinds = Some(Ok(vec![question_form(), both_form(), delivery_form()]));
        (held, s)
    }

    /// The Kinds page with the question's card and the par's open, each its text beside its picture.
    fn kinds_page() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = stand_in();
        s.sel = crate::Sel::Kinds;
        s.kinds_open.insert("question".into());
        s.kinds_open.insert("both".into());
        (held, s)
    }

    /// The Kinds page with delivery's card the one open, its text beside its picture.
    fn delivery_page() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = stand_in();
        s.sel = crate::Sel::Kinds;
        s.kinds_open.insert("delivery".into());
        (held, s)
    }

    /// Ticket 7's page with its whole program open, lit: apart, as its page opens it (popup.rs).
    fn ticket_page() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = stand_in();
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        // titled as its page's press titles it (ticket.rs program_part)
        let title = crate::popup::title(&crate::refs::ticket(Some(&s.projects[0]), &s.projects[0].live.tickets[&7]).label, "N-7", "the whole program, every branch");
        crate::popup::open(&mut s, title, crate::popup::What::Program { n: 7 });
        (held, s)
    }

    /// N-1396 p282, 1: on ticket 7's page "› the whole program, every branch" opens it apart, lit, the page's stages
    /// left as they were; a step picked in it has its panel beside it there, and beside the page only once it is closed.
    #[test]
    fn the_whole_program_opens_apart_with_a_picked_steps_panel_beside_it() {
        let (_held, mut s) = stand_in();
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        let mut rt = Rt::new(s, crate::view, (1400.0, 2400.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        let els = rt.elements();
        let under = |els: &[rune::ElementInfo], key: &str, root: &str| els.iter().filter(|e| !e.exiting && e.key.as_deref() == Some(key) && e.path.split('/').any(|k| k == root)).count();
        let line = els.iter().find(|e| !e.exiting && e.text.as_deref() == Some("› the whole program, every branch")).map(|e| e.rect).expect("the line under the stages");
        assert_eq!(under(&els, "picture", "ticket-detail"), 1, "the stages on the page");
        assert_eq!(under(&els, "pic-N-7.1", "ticket-detail"), 0, "the whole program not on the page");
        let (x, y) = (line[0] + line[2] / 2.0, line[1] + line[3] / 2.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(&mut rt);
        assert_eq!(rt.state.popups.len(), 1);
        assert_eq!(rt.state.popups[0].what, crate::popup::What::Program { n: 7 });
        let title = &rt.state.popups[0].title;
        assert!(title.ends_with(" · N-7 · the whole program, every branch"), "{title}");
        let els = rt.elements();
        assert_eq!(under(&els, "pic-N-7.1", "picture-whole"), 1, "the whole program, lit, apart");
        assert_eq!(under(&els, "picture-whole", "popup0-dialog"), 1);
        let line_now = els.iter().find(|e| !e.exiting && e.path.split('/').any(|k| k == "ticket-detail") && e.text.as_deref() == Some("› the whole program, every branch")).map(|e| e.rect);
        assert_eq!(line_now, Some(line), "the page under it as it was");
        rt.state.tickets.edits.step = Some(crate::edit::Step { t: 7, key: "pic-N-7.1".into(), worker: "f5-2".into(), call: "mind".into(), inline: None, awaited: true });
        settle(&mut rt);
        let els = rt.elements();
        assert_eq!(els.iter().filter(|e| !e.exiting && e.key.as_deref() == Some("step-panel")).count(), 1, "the step's panel drawn once");
        assert_eq!(under(&els, "step-panel", "popup0-dialog"), 1, "beside the program apart");
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: rune::Mods::default() });
        settle(&mut rt);
        assert!(rt.state.popups.is_empty());
        let els = rt.elements();
        assert_eq!(under(&els, "step-panel", "stage-panel"), 1, "closed apart, the panel beside the page");
    }

    /// The window's own page S, WIDE points wide, with the match or par PART of a picture in ROOM (its card, or the
    /// ticket page's picture): ROOM stays in the window and PART's visible part in ROOM; every text of PART stands
    /// inside each box round it, up to PART; no word of a text is wider than the text (a `return`'s in its pill's
    /// small_medium, the rest in small). Wider than its room, PART scrolls: a wheel across it brings its far end into
    /// ROOM. The texts seen, and whether it scrolled.
    fn fits(s: App, wide: f32, room: &str, part: &str) -> (usize, bool) {
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (wide, 3200.0), rune::Env::new(false, 1.0));
        settle(&mut rt);
        let k = rune_kit::Kit::neutral(false);
        let els = rt.elements();
        let at = |key: &str| els.iter().position(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("no {key} at {wide}"));
        let (room_i, pic) = (at(room), at(part));
        let inside = |r: [f32; 4], b: [f32; 4]| r[0] >= b[0] - 0.5 && r[0] + r[2] <= b[0] + b[2] + 0.5;
        let (r, p, v) = (els[room_i].rect, els[pic].rect, els[pic].visible);
        assert!(r[0] + r[2] <= wide + 0.5, "{room} {r:?} runs past the window, {wide} wide");
        assert!(inside(v, r), "at {wide}: {part}'s visible {v:?} runs past {room} {r:?}");
        // the boxes round element I, up to PART; none when it is not in PART
        let round = |mut i: usize| {
            let mut up = Vec::new();
            while let Some(p) = els[i].parent {
                up.push(p);
                if p == pic {
                    return up;
                }
                i = p;
            }
            Vec::new()
        };
        let mut seen = 0;
        for (i, e) in els.iter().enumerate() {
            let (Some(t), false) = (e.text.as_deref(), e.exiting) else { continue };
            let up = round(i);
            if up.is_empty() {
                continue;
            }
            seen += 1;
            for &b in &up {
                assert!(inside(e.rect, els[b].rect), "at {wide}: {t:?} at {:?} runs past {:?} at {:?}", e.rect, els[b].key, els[b].rect);
            }
            let style = if t.starts_with("return") { k.ty.small_medium } else { k.ty.small };
            for w in t.split_whitespace() {
                let wide_w = crate::picture::graph::measure(w, style).0;
                assert!(wide_w <= e.rect[2] + 0.5, "at {wide}: {w:?} of {t:?} is {wide_w} wide, in a text {} wide", e.rect[2]);
            }
        }
        let scrolls = p[2] > v[2] + 0.5;
        if scrolls {
            // a wheel across its visible part: its far end comes into the room
            rt.wheel_xy(v[0] + v[2] / 2.0, v[1] + v[3].min(40.0) / 2.0, -(p[2] - v[2] + 40.0), 0.0);
            for _ in 0..60 {
                rt.frame(1.0 / 60.0);
            }
            settle(&mut rt);
            let p2 = rect(&rt, part).unwrap();
            assert!(p2[0] < p[0] - 1.0 && p2[0] + p2[2] <= r[0] + r[2] + 0.5, "at {wide}: {part} {p:?} scrolled to {p2:?}, its end not in {room} {r:?}");
        }
        println!("{wide}: {part} {:.0} wide, {:.0} shown in {room} {:.0}; {seen} texts", p[2], v[2], r[2]);
        (seen, scrolls)
    }

    /// The Kinds page's three cards and a lit ticket page, in the window the stills are taken in and in the window's
    /// smallest (720, lib.rs `min_size`): no text runs past its box, and a picture wider than its room scrolls in it,
    /// never past it; where its arms can wrap to the room it does not scroll (rune's `w_min_content`). At 42a51762 a short arm
    /// was only as wide as its case, and its `return Answered(text)` ran into its pill's border (consult-3, N-1396
    /// p153); at 76f36bb9 the arms kept their width and the match ran past its card at 720 (p163); a plain `scroll_x`
    /// laid the question out unwrapped, 604 wide in its 530, and it scrolled at 1400. At a67981a5 delivery's picture
    /// ran past its card's right edge at 1400 (docket-edge8, p172): its `when`s, in a wrapping row that never shrank
    /// them, were each as wide as its arms unwrapped, "⇢ when arrived(e)"'s 815 in 530; grown from nothing, they fit at
    /// 1400 and ran past at 720, 353 in 272, as taffy counts nothing of them in a wrapping row's minimum.
    #[test]
    fn on_the_kinds_card_no_text_runs_past_its_box() {
        // each page's App made with the lock held while it is drawn
        let on = |page: fn() -> (std::sync::MutexGuard<'static, ()>, App), wide: f32, room: &str, part: &str| {
            let (_held, s) = page();
            fits(s, wide, room, part)
        };
        for wide in [1400.0, 720.0] {
            let (seen, scrolls) = on(kinds_page, wide, "kind-question", "pic-kind-question.1");
            assert!(seen >= 12, "the question's texts at {wide}: {seen}");
            assert_eq!(scrolls, wide < 1000.0, "the question scrolls at {wide}: {scrolls}");
            let (seen, _) = on(kinds_page, wide, "kind-both", "pic-kind-both.0");
            assert!(seen >= 4, "the par's texts at {wide}: {seen}");
            let (seen, _) = on(delivery_page, wide, "kind-delivery", "pic-kind-delivery");
            assert!(seen >= 16, "delivery's texts at {wide}: {seen}");
        }
        let (seen, _) = on(ticket_page, 720.0, "picture-whole", "pic-N-7.1");
        assert!(seen >= 8, "the lit question at 720: {seen} texts");
    }

    /// STILL_OUT=DIR: the Kinds page's question and par cards open, and delivery's (each its text beside its picture), and ticket 7's page with its
    /// whole program open apart, lit; each cut to its part (ticket-apart: the popup whole), made-up data.
    #[test]
    #[ignore]
    fn wire_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn kinds() -> App {
            kinds_page().1
        }
        fn ticket() -> App {
            ticket_page().1
        }
        fn delivery() -> App {
            delivery_page().1
        }
        let shots: [(&str, fn() -> App, &str, f32); 8] = [
            ("kinds-question", kinds, "kind-question", 1400.0),
            ("kinds-question-720", kinds, "kind-question", 720.0),
            ("kinds-both-720", kinds, "kind-both", 720.0),
            ("kinds-delivery", delivery, "kind-delivery", 1400.0),
            ("kinds-delivery-720", delivery, "kind-delivery", 720.0),
            ("ticket-lit", ticket, "picture-whole", 1400.0),
            ("ticket-lit-720", ticket, "picture-whole", 720.0),
            ("ticket-apart", ticket, "popup0-dialog", 1400.0),
        ];
        for (name, setup, crop, wide) in shots {
            let snap = out.join(format!("{name}.png")).display().to_string();
            let size = format!("{wide}x2200");
            let args: [&str; 8] = ["--snap", &snap, "--crop", crop, "--size", &size, "--at", "0.5"];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (wide, 2200.0), setup, crate::view) };
            rune::App::new(spec).args(args.map(String::from).to_vec()).try_run().unwrap();
        }
    }
}
