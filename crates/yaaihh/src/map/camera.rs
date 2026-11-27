//! The Map's camera: which world point is at the view's top-left and how many screen points one world unit takes. A
//! zoom keeps the point under the pointer where it is, as an open image's zoom does (viewer.rs `zoom_about`: the
//! pixel under P stays under it); a drag moves the world with the pointer; a jump to a node flies along van Wijk and
//! Nuij's path ("Smooth and efficient zooming and panning", 2003; d3's `interpolateZoom`): out, across and in as one
//! move, so the eye keeps its bearings. A flight always flies: the window sets its own motion, not the system's
//! (lib.rs `view`: `reduced_motion(false)`), so Windows' animations off does not make it a cut.

use super::Rect;

/// The camera. `at` is the world point at the view's top-left; `scale` is screen points per world unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub at: [f64; 2],
    pub scale: f64,
}

impl Default for Camera {
    fn default() -> Camera {
        Camera { at: [0.0, 0.0], scale: 1.0 }
    }
}

/// The nearest and the farthest a camera goes: a call on the wire read whole, a few thousand projects at once.
pub const NEAREST: f64 = 1.0e4;
pub const FARTHEST: f64 = 1.0e-5;

impl Camera {
    /// World point P on screen, in the view's own points.
    pub fn to_screen(&self, p: [f64; 2]) -> [f32; 2] {
        [((p[0] - self.at[0]) * self.scale) as f32, ((p[1] - self.at[1]) * self.scale) as f32]
    }

    /// The world point under Q, a point of the view.
    pub fn to_world(&self, q: [f32; 2]) -> [f64; 2] {
        [self.at[0] + q[0] as f64 / self.scale, self.at[1] + q[1] as f64 / self.scale]
    }

    /// R on screen: [x, y, w, h] in the view's points.
    pub fn on_screen(&self, r: &Rect) -> [f32; 4] {
        let [x, y] = self.to_screen([r.x, r.y]);
        [x, y, (r.w * self.scale) as f32, (r.h * self.scale) as f32]
    }

    /// The world a view of VIEW points shows.
    pub fn seen(&self, view: [f32; 2]) -> Rect {
        Rect::new(self.at[0], self.at[1], view[0] as f64 / self.scale, view[1] as f64 / self.scale)
    }

    /// Zoom by FACTOR about Q, a point of the view: the world point under Q stays under it.
    pub fn zoom_about(&mut self, factor: f64, q: [f32; 2]) {
        let w = self.to_world(q);
        self.scale = (self.scale * factor).clamp(FARTHEST, NEAREST);
        self.at = [w[0] - q[0] as f64 / self.scale, w[1] - q[1] as f64 / self.scale];
    }

    /// The world moved by D view points, as a drag moves it: what was under the pointer stays under it.
    pub fn shift(&mut self, d: [f32; 2]) {
        self.at = [self.at[0] - d[0] as f64 / self.scale, self.at[1] - d[1] as f64 / self.scale];
    }

    /// The camera that shows R whole in a view of VIEW points, centred, with ROOM points around it.
    pub fn fitting(r: &Rect, view: [f32; 2], room: f32) -> Camera {
        let (vw, vh) = ((view[0] - 2.0 * room).max(1.0) as f64, (view[1] - 2.0 * room).max(1.0) as f64);
        let scale = (vw / r.w.max(1e-12)).min(vh / r.h.max(1e-12)).clamp(FARTHEST, NEAREST);
        let [mx, my] = r.mid();
        Camera { at: [mx - view[0] as f64 / 2.0 / scale, my - view[1] as f64 / 2.0 / scale], scale }
    }

    /// The world point at the view's middle and the width of world it shows: the camera as the flight sees it.
    fn as_view(&self, view: [f32; 2]) -> [f64; 3] {
        let w = view[0] as f64 / self.scale;
        [self.at[0] + w / 2.0, self.at[1] + view[1] as f64 / self.scale / 2.0, w]
    }

    fn of_view(v: [f64; 3], view: [f32; 2]) -> Camera {
        let scale = view[0] as f64 / v[2];
        Camera { at: [v[0] - v[2] / 2.0, v[1] - view[1] as f64 / scale / 2.0], scale }
    }
}

/// A flight from one camera to another along van Wijk and Nuij's path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flight {
    from: [f64; 3],
    to: [f64; 3],
    view: [f32; 2],
    /// The path's length (van Wijk and Nuij's S), in their units.
    s: f64,
    r0: f64,
    d1: f64,
}

/// ρ, the path's balance of zooming against panning: √2, as van Wijk and Nuij advise and d3 has it.
const RHO: f64 = std::f64::consts::SQRT_2;

impl Flight {
    pub fn new(from: Camera, to: Camera, view: [f32; 2]) -> Flight {
        let (a, b) = (from.as_view(view), to.as_view(view));
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let d2 = dx * dx + dy * dy;
        let d1 = d2.sqrt();
        if d2 < 1e-12 * a[2] * a[2] {
            return Flight { from: a, to: b, view, s: (b[2] / a[2]).ln().abs() / RHO, r0: 0.0, d1: 0.0 };
        }
        let rho2 = RHO * RHO;
        let b0 = (b[2] * b[2] - a[2] * a[2] + rho2 * rho2 * d2) / (2.0 * a[2] * rho2 * d1);
        let b1 = (b[2] * b[2] - a[2] * a[2] - rho2 * rho2 * d2) / (2.0 * b[2] * rho2 * d1);
        let r0 = ((b0 * b0 + 1.0).sqrt() - b0).ln();
        let r1 = ((b1 * b1 + 1.0).sqrt() - b1).ln();
        Flight { from: a, to: b, view, s: (r1 - r0) / RHO, r0, d1 }
    }

    /// How long it takes, in seconds: d3's, a second per unit of the path, held between a fifth of a second and nine
    /// tenths so a near jump is seen and a far one does not drag.
    pub fn seconds(&self) -> f64 {
        self.s.abs().clamp(0.2, 0.9)
    }

    /// The camera at T, from 0 (where it left) to 1 (where it lands).
    pub fn at(&self, t: f64) -> Camera {
        let t = t.clamp(0.0, 1.0);
        if t >= 1.0 {
            return Camera::of_view(self.to, self.view);
        }
        let (a, b) = (self.from, self.to);
        let v = if self.d1 == 0.0 {
            // the same middle: only the width changes, at a steady rate of zoom
            [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1]), a[2] * (b[2] / a[2]).powf(t)]
        } else {
            let s = t * self.s;
            let (c0, rho2) = (self.r0.cosh(), RHO * RHO);
            let u = a[2] / (rho2 * self.d1) * (c0 * (RHO * s + self.r0).tanh() - self.r0.sinh());
            [a[0] + u * (b[0] - a[0]), a[1] + u * (b[1] - a[1]), a[2] * c0 / (RHO * s + self.r0).cosh()]
        };
        Camera::of_view(v, self.view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn a_zoom_keeps_the_point_under_the_pointer() {
        let mut c = Camera { at: [120.0, -40.0], scale: 0.25 };
        let q = [312.0, 207.0];
        let under = c.to_world(q);
        for f in [1.1, 3.0, 0.5, 40.0] {
            c.zoom_about(f, q);
            let w = c.to_world(q);
            assert!(near(w[0], under[0]) && near(w[1], under[1]), "{f}: {w:?} not {under:?}");
        }
        let before = c.to_world([0.0, 0.0]);
        c.shift([30.0, -10.0]);
        let after = c.to_world([30.0, -10.0]);
        assert!(near(before[0], after[0]) && near(before[1], after[1]), "a drag moves the world with the pointer");
    }

    #[test]
    fn a_fit_shows_the_rect_whole_and_centred() {
        let r = Rect::new(5_000.0, 2_000.0, 300.0, 100.0);
        let view = [1200.0, 800.0];
        let c = Camera::fitting(&r, view, 20.0);
        let [x, y, w, h] = c.on_screen(&r);
        assert!((w - 1160.0).abs() < 0.01, "as wide as the view less its room: {w}");
        assert!((x - 20.0).abs() < 0.01 && ((y + h / 2.0) - 400.0).abs() < 0.01, "centred: {x} {y} {h}");
    }

    #[test]
    fn a_flight_leaves_where_it_was_and_lands_where_it_goes() {
        let view = [1200.0, 800.0];
        let from = Camera::fitting(&Rect::new(0.0, 0.0, 10_000.0, 8_000.0), view, 0.0);
        let to = Camera::fitting(&Rect::new(7_000.0, 6_000.0, 12.0, 4.0), view, 0.0);
        let f = Flight::new(from, to, view);
        let (a, b) = (f.at(0.0), f.at(1.0));
        assert!(near(a.scale, from.scale) && near(a.at[0], from.at[0]) && near(a.at[1], from.at[1]), "{a:?}");
        assert_eq!(b, Camera::of_view(to.as_view(view), view));
        assert!(near(b.scale, to.scale), "{b:?} {to:?}");
        // a far jump goes out, across and in: never nearer than where it lands, the scale only rising at the end
        let mid = f.at(0.5);
        assert!(mid.scale < to.scale && mid.scale > 0.0, "{mid:?}");
        assert!((0.2..=0.9).contains(&f.seconds()));
        // the same middle, nearer: a steady zoom
        let mut nearer = from;
        nearer.zoom_about(8.0, [600.0, 400.0]);
        let z = Flight::new(from, nearer, view);
        let m = z.at(0.5);
        assert!(near(m.scale, from.scale * 8f64.sqrt()), "halfway is the zoom's halfway in steps: {m:?}");
        assert!(near(m.to_world([600.0, 400.0])[0], 5_000.0), "about the same middle");
    }
}
