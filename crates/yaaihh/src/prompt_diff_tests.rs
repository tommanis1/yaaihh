//! The system prompt read against Claude Code's own: what is left out, changed and added, in order, and what each
//! costs; a pill left out opens to its text and Restore, which drafts it back in the parts editor.
use super::*;
use crate::App;
use rune::runtime::Runtime;

fn part(name: &str, text: &str, ours: bool) -> Part {
    Part { name: name.into(), text: text.into(), ours }
}

fn standing(from: &str, text: &str) -> StandingPart {
    StandingPart { from: from.into(), path: format!("/{from}/standing.md"), text: text.into() }
}

const GUIDANCE: &str = "# Session-specific guidance\n - If you need the user to run a shell command themselves, suggest they type `! <command>`.";

fn stock() -> Vec<Part> {
    vec![part("Agent line", "You are an agent.", false), part("Pronouns", "When you use a pronoun for someone, use they/them.", false), part("Session-specific guidance", GUIDANCE, false), part("Environment", "# Environment\n - Linux", false)]
}

/// His version in place of Claude Code's own: Pronouns and the guidance left out, Environment in his words, a part of
/// his own, and the standing texts after it.
fn sent() -> Vec<Part> {
    vec![part("Agent line", "You are an agent.", false), part("Environment", "# Environment\n - Linux, on the tower", false), part("Mine", "Answer in short lines.", false), part("ours", "For every agent.\n\nOnly here.", true)]
}

#[test]
fn what_is_left_out_changed_and_added_stands_in_order_with_its_tokens() {
    let files = [standing("global", "For every agent.\n"), standing("project", "Only here.\n")];
    let d = diff(Some("You are Claude Code."), &stock(), &sent(), &files, false);
    let rows: Vec<String> = d
        .rows
        .iter()
        .map(|r| match r {
            Row::Same(x) => format!("= {}", x.text),
            Row::Stripped { name, restorable: true, .. } => format!("- {name}"),
            Row::Stripped { name, .. } => format!("-! {name}"),
            Row::Changed { name, .. } => format!("~ {name}"),
            Row::Added(x) => format!("+ {} {}", x.source.words(), x.text),
        })
        .collect();
    assert_eq!(rows, ["= You are Claude Code.", "= You are an agent.", "- Pronouns", "- Session-specific guidance", "~ Environment", "+ agent override Answer in short lines.", "+ global standing.md For every agent.", "+ project standing.md Only here."]);
    let t = |x: &str| tokens(x);
    assert_eq!(d.base, t("You are Claude Code.") + stock().iter().map(|p| t(&p.text)).sum::<usize>());
    assert_eq!(d.minus, t(&stock()[1].text) + t(GUIDANCE) + t("# Environment\n - Linux"));
    assert_eq!(d.plus, t("# Environment\n - Linux, on the tower") + t("Answer in short lines.") + t("For every agent.") + t("Only here."));
    assert_eq!(d.sum(), format!("Base {} − {} + {} = {} tok", d.base, d.minus, d.plus, d.base - d.minus + d.plus));
    assert_eq!((tokens(&"x".repeat(568)), short(142), short(14_200), short(13_049)), (142, "142".into(), "14.2k".into(), "13.0k".into()));
    let odd = Diff { base: 10, minus: 40, plus: 5, ..Diff::default() };
    assert_eq!(odd.sum(), "Base 10 − 40 + 5 = 0 tok", "a file that cuts more than its base adds up to nothing, no panic");
    let same = diff(None, &stock(), &stock(), &[], false);
    assert!(same.rows.iter().all(|r| matches!(r, Row::Same(_))) && same.minus == 0 && same.plus == 0, "nothing changed: nothing marked");
}

fn shown(s: &App, _: &mut rune::Cx<App>) -> rune::El {
    let k = crate::look::kit(s, true);
    let d = diff(None, &stock(), &sent(), &[], false);
    rune::col().w_full().padding(16.0).children(drawn(s, 0, "lead", &d, &k))
}

fn settle(rt: &mut Runtime<App>) {
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

fn press(rt: &mut Runtime<App>, key: &str) {
    let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
    let (x, y) = (r[0] + 8.0, r[1] + r[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    settle(rt);
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
}

/// Drawn: the sum first; each part left out a pill of what it saves and its first line, its text not shown; a press
/// opens it, Restore drafts it back (Stock) in the parts editor and becomes Undo restore, which drops the draft.
#[test]
fn a_pill_opens_to_its_text_and_restore_drafts_it_back() {
    let (_held, s) = crate::view_cost::test_app();
    let mut rt = Runtime::new(s, shown, (900.0, 900.0), rune::Env::new(false, 1.0));
    settle(&mut rt);
    let t = texts(&rt);
    assert!(t[0].starts_with("Base ") && t[0].ends_with(" tok"), "{t:?}");
    let g = tokens(GUIDANCE);
    assert!(t.iter().any(|x| *x == format!("−{g} tok · Session-specific guidance")), "{t:?}");
    assert!(t.iter().any(|x| x == "# Session-specific guidance") && !t.iter().any(|x| x.contains("suggest they type")), "its first line only: {t:?}");
    assert!(t.iter().any(|x| x == "+"), "an addition's gutter");
    press(&mut rt, "prompt-pill-Session-specific guidance");
    assert!(texts(&rt).iter().any(|x| x.contains("suggest they type")), "opened: its text");
    press(&mut rt, "prompt-restore-Session-specific guidance");
    let drafted = |rt: &Runtime<App>| crate::prompt_edit::draft_of(&rt.state, 0, "lead").and_then(|p| p.get("Session-specific guidance")).map(|(p, _)| *p);
    assert_eq!(drafted(&rt), Some(Pick::Stock), "drafted back in");
    assert!(texts(&rt).iter().any(|x| x == "Undo restore"));
    press(&mut rt, "prompt-restore-Session-specific guidance");
    assert_eq!(drafted(&rt), None, "the draft dropped");
}

/// Worked out once while what it reads is the same, again when that changes.
#[test]
fn a_document_is_worked_out_again_only_when_what_it_reads_changes() {
    let made = std::cell::Cell::new(0);
    let make = || {
        made.set(made.get() + 1);
        diff(None, &stock(), &sent(), &[], false)
    };
    let (a, b) = (key_of(&("one", 1)), key_of(&("two", 1)));
    assert_eq!(a, key_of(&("one", 1)));
    kept(7, "lead", a, make);
    kept(7, "lead", a, make);
    assert_eq!(made.get(), 1, "the same: kept");
    kept(7, "lead", b, make);
    assert_eq!(made.get(), 2, "changed: worked out again");
}
