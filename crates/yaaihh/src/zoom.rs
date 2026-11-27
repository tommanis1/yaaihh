//! The whole window's zoom (owner m188, 9 Nov): Ctrl+wheel, Ctrl+= and Ctrl+- in tenths, Ctrl+0 back to 100%, and the
//! toolbar's − / % / + buttons (`toolbar`), kept in
//! ui.json (state.rs). Rune lays the window out, draws its text and hit-tests at the system's scale times the zoom
//! (`Tx::set_ui_zoom`), so every page follows without knowing.

use crate::App;
use rune::{Key, KeyEvent, Tx, WheelEvent};

/// Steps per 100%: a step is a tenth.
const STEPS: f32 = 10.0;
const STEP: f32 = 1.0 / STEPS;
const MIN: f32 = 0.5;
const MAX: f32 = 3.0;
/// Wheel points per step: one notch of a mouse wheel (Rune's 40 points a line); a trackpad's small deltas add up to it.
const NOTCH: f32 = 40.0;

/// The window's zoom and the wheel's delta not yet a whole step.
#[derive(Default)]
pub struct Zoom {
    /// 1.0 is 100%; 0.0 until set means 100% too (a fresh App, a ui.json without one).
    level: f32,
    /// The level Rune was last given.
    shown: f32,
    wheel: f32,
}

impl Zoom {
    pub fn level(&self) -> f32 {
        if self.level == 0.0 { 1.0 } else { self.level }
    }

    /// Kept from ui.json: in range and on a step, or 100% for anything else.
    pub fn restore(&mut self, level: f32) {
        self.level = if level.is_finite() { snap(level) } else { 1.0 };
    }

    fn set(&mut self, level: f32, tx: &mut Tx) {
        self.level = snap(level);
        self.show(tx);
    }

    /// Gives Rune the level when it has another one (the restored level, at the first poll).
    pub fn show(&mut self, tx: &mut Tx) {
        let level = self.level();
        if level != self.shown {
            self.shown = level;
            tx.set_ui_zoom(level);
        }
    }
}

/// In range, rounded to a whole step, so steps up and down come back to 100%.
fn snap(level: f32) -> f32 {
    ((level * STEPS).round() / STEPS).clamp(MIN, MAX)
}

/// The top bar's `⋯`, lean (N-1860): the page's own Spin off and Pins, Actual size while zoomed, Dark mode, Split,
/// Vocabulary (vocab.rs) and Restart window. Each zoom gives Rune the level at once (owner m231).
pub fn menu(s: &App, k: &rune_kit::Kit, layer: &mut Vec<rune::El>) -> rune::El {
    use rune_kit::{Menu, MenuItem};
    use rune_kit::{Button, ButtonVariant};
    let level = s.zoom.level();
    let dark = k.dark;
    // the bar's one quiet control (docs/specs/window-rules.md §1): the zoom is felt, not written on the bar; Spin off and Pin of
    // the page shown are here too, not buttons of their own (paper-research N-873)
    let trigger = Button::new("⋯").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.bar_menu = !s.bar_menu).build(k);
    let mut m = Menu::new("bar-menu")
        .trigger(trigger)
        .open(s.bar_menu)
        .placement(rune_kit::Placement::BottomEnd)
        .on_close(|s: &mut App| s.bar_menu = false);
    if let crate::Sel::Agent(name) = &s.sel {
        let forking = s.proj().is_some_and(|p| p.forking.is_some());
        let name = name.clone();
        m = m.item(MenuItem::new(if forking { "Spinning off…".to_string() } else { format!("Spin off {name}") }).disabled(forking || name.is_empty()).on_select(move |s: &mut App| {
            crate::projects::fork(s, &name, None);
            s.bar_menu = false;
        }));
    }
    if let Some(pin) = s.shown_pin() {
        let on = s.pins.has(&pin);
        m = m.item(MenuItem::new(if on { "Unpin" } else { "Pin to the sidebar" }).on_select(move |s: &mut App| {
            s.pins.toggle(pin.clone());
            s.bar_menu = false;
        }));
    }
    // the views of the page shown as lenses of their own in Pinned (lens.rs `offers`)
    let offers = crate::lens::offers(s);
    for pin in offers.iter().filter_map(|l| s.lens_pin(l)) {
        let on = s.pins.has(&pin);
        let name = pin.agent.clone();
        m = m.item(MenuItem::new(if on { format!("Unpin {name}") } else { format!("Pin {name}") }).on_select(move |s: &mut App| {
            s.pins.toggle(pin.clone());
            s.bar_menu = false;
        }));
    }
    if matches!(s.sel, crate::Sel::Agent(_)) || s.shown_pin().is_some() || !offers.is_empty() {
        m = m.separator();
    }
    // zoom is Ctrl+=/-/0 and Ctrl+wheel; here only the way back to 100% while zoomed
    if (level - 1.0).abs() >= 0.01 {
        m = m.item(MenuItem::new(format!("Actual size (now {:.0}%)", level * 100.0)).shortcut("Ctrl+0").on_select_tx(|s: &mut App, tx: &mut Tx| {
            s.zoom.set(1.0, tx);
            s.bar_menu = false;
        }));
    }
    let m = m
        .item(MenuItem::new("Dark mode").checked(dark).on_select(move |s: &mut App| {
            s.dark = Some(!dark);
            s.bar_menu = false;
        }))
        .item(MenuItem::new("Split: a second pane beside this one").disabled(!s.can_split()).on_select(|s: &mut App| {
            s.split();
            s.bar_menu = false;
        }))
        .item(MenuItem::new("Vocabulary").on_select(|s: &mut App| {
            crate::vocab::open(s);
            s.bar_menu = false;
        }));
    // the window started again, as a release starts it: it comes back as he left it (restart.rs `now`; N-1793, item 3)
    let m = if crate::restart::can_restart(s) {
        m.separator().item(MenuItem::new("Restart window").on_select_tx(|s: &mut App, tx: &mut Tx| {
            s.bar_menu = false;
            crate::restart::now(s, tx);
        }))
    } else {
        m
    };
    m.build(k, layer)
}

/// Ctrl+= (and Ctrl++, the numpad's +), Ctrl+- and Ctrl+0; whether the key was one of them.
pub fn key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Key::Command(c) = ev.key else { return false };
    let z = &mut s.zoom;
    let to = match c {
        '=' | '+' => z.level() + STEP,
        '-' | '_' => z.level() - STEP,
        '0' => 1.0,
        _ => return false,
    };
    z.set(to, tx);
    true
}

/// Ctrl+wheel anywhere in the window (Rune's app-level `on_wheel`, ahead of scroll views): up zooms in, down out, a step
/// a notch. Takes it, so nothing scrolls under it; over an element that takes it itself (`App::wheel_taken`) it passes.
pub fn wheel(s: &mut App, e: &WheelEvent, tx: &mut Tx) -> bool {
    // an open image takes the wheel: Ctrl zooms it, not the window
    if crate::viewer::wheel(s, e) {
        return true;
    }
    if !e.mods.ctrl || s.wheel_taken {
        return false;
    }
    let z = &mut s.zoom;
    z.wheel += e.dy;
    let steps = (z.wheel / NOTCH).trunc();
    if steps != 0.0 {
        z.wheel -= steps * NOTCH;
        z.set(z.level() + steps * STEP, tx);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_come_back_to_100_and_stay_in_range() {
        let mut level = 1.0;
        for _ in 0..7 {
            level = snap(level + STEP);
        }
        for _ in 0..7 {
            level = snap(level - STEP);
        }
        assert_eq!(level, 1.0);
        assert_eq!(snap(0.1), MIN);
        assert_eq!(snap(9.0), MAX);
        assert_eq!(snap(1.26), 1.3);
    }

    #[test]
    fn a_missing_level_is_100() {
        let mut z = Zoom::default();
        assert_eq!(z.level(), 1.0);
        z.restore(f32::NAN);
        assert_eq!(z.level(), 1.0);
        z.restore(1.2);
        assert!((z.level() - 1.2).abs() < 1e-6);
    }
}
