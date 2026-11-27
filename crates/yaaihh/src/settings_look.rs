//! The look the Settings page and the v2 Policy page share: the levels listed on the left, one row per setting with
//! its control at the right end, an inherited value muted with where it comes from, a choice of words.

use rune::{col, row, text, El, ScrollSpec};
use rune_kit::{Kit, SegmentedControl};

/// The page keyed KEY: LEVELS on the left, TITLE and BODY on the right, scrolled as one.
pub fn page(key: &str, levels: El, title: &str, body: El, k: &Kit) -> El {
    let right = col().grow(1.0).shrink().min_w(0.0).gap(4.0).child(text(title.to_string()).font(k.ty.ui_medium).color(k.c.text_base).pb(8.0)).child(body);
    col()
        .key(key)
        .w_full()
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(row().w_full().max_w(1040.0).gap(32.0).px(24.0).py(20.0).items_start().child(levels).child(right))
}

/// The list the levels go in, keyed KEY.
pub fn levels(key: &str) -> El {
    col().key(key).w(200.0).gap(2.0)
}

/// A heading among the levels.
pub fn heading(t: &str, k: &Kit) -> El {
    text(t.to_string()).font(k.ty.small_medium).color(k.c.text_faint).pt(12.0).pb(4.0).px(10.0)
}

/// One level, keyed KEY and drawn picked when ON; a press runs GO.
pub fn level<S: 'static>(key: String, name: &str, on: bool, go: impl Fn(&mut S) + 'static, k: &Kit) -> El {
    let name = text(name.to_string()).font(k.ty.body).color(if on { k.c.text_base } else { k.c.text_muted }).truncate().shrink();
    let el = row().w_full().px(10.0).py(6.0).radius(k.radius.md).child(name);
    let el = if on { el.fill(k.c.layer_02) } else { el };
    k.interactive(el.key(key)).on_press(go)
}

/// A heading among the settings.
pub fn section(t: &str, k: &Kit) -> El {
    text(t.to_string()).font(k.ty.small_medium).color(k.c.text_muted).pt(20.0).pb(4.0)
}

/// One setting's row, keyed KEY: LABEL with ABOUT under it on the left, RIGHT at the right end.
pub fn setting(key: String, label: &str, about: &str, right: El, k: &Kit) -> El {
    let left = col()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(2.0)
        .child(text(label.to_string()).font(k.ty.body).color(k.c.text_base))
        .children((!about.is_empty()).then(|| text(about.to_string()).font(k.ty.small).color(k.c.text_faint).wrap()));
    row().key(key).w_full().gap(16.0).center_y().py(12.0).border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak).child(left).child(right)
}

/// An inherited value, muted, with where it comes from under it.
pub fn faint_value(k: &Kit, value: &str, tag: &str) -> El {
    let words = col().items_end().child(text(value.to_string()).font(k.ty.body).color(k.c.text_faint)).child(text(tag.to_string()).font(k.ty.small).color(k.c.text_faint));
    row().gap(12.0).center_y().child(words)
}

/// A choice NAMED among LABELS, PICKED drawn picked; picking another runs PICK with its place.
pub fn choice<S: 'static, L: AsRef<str>>(named: &str, labels: &[L], picked: Option<usize>, pick: impl Fn(&mut S, usize) + Clone + 'static, k: &Kit) -> El {
    let ctl = SegmentedControl::new(labels, picked).label(named).fit().on_pick(move |s: &mut S, j, _| {
        if Some(j) != picked {
            pick(s, j);
        }
    });
    row().gap(12.0).center_y().child(ctl.build(k))
}
