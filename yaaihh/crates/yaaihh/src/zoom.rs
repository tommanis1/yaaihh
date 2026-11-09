//! The whole window's zoom: Ctrl+wheel, Ctrl+= and Ctrl+- in tenths, Ctrl+0 back to 100%, kept in
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

/// Ctrl+wheel anywhere in the window: up zooms in, down out, a step a notch. Takes it, so nothing scrolls under it.
pub fn wheel(s: &mut App, e: &WheelEvent, tx: &mut Tx) -> bool {
    if !e.mods.ctrl {
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
