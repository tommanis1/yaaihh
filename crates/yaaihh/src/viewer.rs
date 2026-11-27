//! The image viewer over the whole window (owner N-313: "add cntl zoom to images to zoom into them and nviagte opened
//! images"). It shows `App::viewing`, an image of `App::images`: a chat's image clicked, or a picture whose path a post
//! or chat names (a click on a `.png`/`.jpg` path opens it here, not in VS Code).
//!
//! - Ctrl with the wheel zooms at the pointer; Ctrl with + and - zooms at the middle; Ctrl 0 fits again.
//! - The wheel alone moves the image (a tall one scrolls); dragging moves it.
//! - On a PDF's page left and right turn its pages; on another image they go to the image opened before and after; with more than one opened, a strip of them is at the
//!   foot, a click on one shows it. Esc, the ×, or a click beside the image closes the viewer.
//! - It opens under the bar, so the bar's arrows stay in reach. Back closes it and goes no further: the arrow, Alt+Left
//!   or the mouse's back button over it (history.rs `step`); going elsewhere closes it with the page it was over (the
//!   walk's 7, paper-research N-1446 p2120).
//!
//! A tall image (taller than the window at the window's width) first shows at the window's width from its top, so a
//! 2560 by 5800 still reads at once; any other fits whole. Neither is drawn larger than its pixels.

use crate::chat::Img;
use crate::App;
use rune::{Key, KeyEvent, Tx, WheelEvent};
use rune_kit::Kit;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Room around the image, and the strip's height when there is one.
const PAD: f32 = 24.0;
const STRIP: f32 = 76.0;
const HEAD: f32 = 36.0;
/// Wheel points per zoom step (one notch), and a step's factor.
const NOTCH: f32 = 40.0;
const FACTOR: f32 = 1.2;
const MAX_ZOOM: f32 = 16.0;
/// The most images the strip keeps (the oldest goes first). A picture read from a file is let go of when it leaves the
/// strip: decoded, a 2560 by 5800 still holds about 80 MB.
const KEEP: usize = 12;

/// What the viewer holds besides `App::viewing`.
#[derive(Default)]
pub struct Shown {
    /// The images opened, in order: the strip, and what Left and Right go through.
    pub opened: Vec<String>,
    /// Times the fitted size (1 is fitted).
    zoom: f32,
    /// The image's top-left in the image area; None: where the fit puts it.
    at: Option<[f32; 2]>,
    /// The image area's rect in window points, as last laid out.
    area: [f32; 4],
    wheel: f32,
    /// Where the image was when the drag began, and whether the drag moved.
    drag_from: [f32; 2],
    dragged: bool,
}

/// Paths opened from a link, waiting for the next poll to read them (`paths::open` runs off the frame).
static QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// The window's waker, so a path opened wakes the poll that reads it.
pub static WAKE: OnceLock<rune::Waker> = OnceLock::new();

/// Whether PATH names a picture the viewer shows.
pub fn is_image(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| lower.ends_with(e))
}

/// Open the picture at PATH in the viewer at the next poll (from any thread).
pub fn open_path(path: PathBuf) {
    QUEUE.lock().unwrap().push(path);
    if let Some(w) = WAKE.get() {
        w();
    }
}

/// The key a picture read from a file is kept under in `App::images`.
pub(crate) fn path_key(p: &std::path::Path) -> String {
    format!("path:{}", p.display())
}

/// Read the paths opened since the last poll and show the last; whether any was.
pub fn drain(s: &mut App) -> bool {
    let paths: Vec<PathBuf> = std::mem::take(&mut *QUEUE.lock().unwrap());
    let mut any = false;
    for p in paths {
        let key = path_key(&p);
        // a picture named by its path is on the WSL machine: asked of coordd (ReadFile, docket-core6's picture rule)
        // when the bridge serves it, its pieces filling it in (`replied`); else the viewer says so in its place
        if crate::elsewhere::away() {
            PIECES.lock().unwrap().remove(&key);
            s.images.insert(key.clone(), Img::Asked);
            if !ask_piece(s, &p, 0) {
                s.images.insert(key.clone(), Img::Failed(crate::elsewhere::LINE.into()));
            }
            show(s, key);
            any = true;
            continue;
        }
        let img = match std::fs::read(&p) {
            Ok(bytes) => Img::Bytes(bytes.into()),
            Err(e) => Img::Failed(e.to_string()),
        };
        s.images.insert(key.clone(), img);
        show(s, key);
        any = true;
    }
    any
}

/// Read the picture at PATH into `App::images` without showing it (a post's picture, thumbs.rs): on the disk here, or
/// asked of coordd in pieces on Windows, as [`drain`] reads one.
pub(crate) fn fetch(s: &mut App, p: &std::path::Path) {
    let key = path_key(p);
    if crate::elsewhere::away() {
        PIECES.lock().unwrap().remove(&key);
        s.images.insert(key.clone(), Img::Asked);
        if !ask_piece(s, p, 0) {
            s.images.insert(key, Img::Failed(crate::elsewhere::LINE.into()));
        }
        return;
    }
    let img = match std::fs::read(p) {
        Ok(bytes) => Img::Bytes(bytes.into()),
        Err(e) => Img::Failed(e.to_string()),
    };
    s.images.insert(key, img);
}

/// Windows: a picture's bytes as its pieces come, by its key.
static PIECES: Mutex<std::collections::BTreeMap<String, Vec<u8>>> = Mutex::new(std::collections::BTreeMap::new());

/// Windows: ask the current project's coordd for the piece of picture PATH from FROM; false when it cannot be asked
/// (a bridge older than the file requests, or the pipe down).
fn ask_piece(s: &mut App, path: &std::path::Path, from: u64) -> bool {
    let Some(p) = s.projects.get_mut(s.cur).filter(|p| crate::files::served(p)) else { return false };
    let req = coord_types::Request::ReadFile { path: crate::files::wire(path), from: Some(from), len: None };
    // a headless scene has no pipe: asked at once
    if rune::is_headless() && p.link.is_none() {
        let (ok, out) = match p.ask_once(&req) {
            Ok(o) => (true, o),
            Err(e) => (false, e),
        };
        replied(s, path.to_path_buf(), from, ok, out);
        return true;
    }
    match p.link.as_mut().and_then(|l| l.ask(&req)) {
        Some(id) => {
            p.pending.insert(id, crate::Pending::Picture { path: path.to_path_buf(), from });
            true
        }
        None => false,
    }
}

/// coordd's answer for the piece of picture PATH from FROM (Windows): the next piece asked, or the picture whole.
pub(crate) fn replied(s: &mut App, path: PathBuf, from: u64, ok: bool, out: String) {
    use base64::Engine;
    #[derive(serde::Deserialize)]
    struct Piece {
        size: u64,
        #[serde(default)]
        data: String,
    }
    let key = path_key(&path);
    let piece = serde_json::from_str::<Piece>(&out).ok().filter(|_| ok);
    let Some((size, data)) = piece.and_then(|p| Some((p.size, base64::engine::general_purpose::STANDARD.decode(&p.data).ok()?))) else {
        PIECES.lock().unwrap().remove(&key);
        s.images.insert(key, Img::Failed(if ok { "unreadable answer".into() } else { out }));
        return;
    };
    let got = {
        let mut all = PIECES.lock().unwrap();
        let buf = all.entry(key.clone()).or_default();
        buf.truncate(from as usize);
        buf.extend_from_slice(&data);
        buf.len() as u64
    };
    if got < size && !data.is_empty() {
        if !ask_piece(s, &path, got) {
            lost(s, &path);
        }
        return;
    }
    let bytes = PIECES.lock().unwrap().remove(&key).unwrap_or_default();
    s.images.insert(key, Img::Bytes(bytes.into()));
}

/// Picture PATH's pieces dropped, the pipe's end said in its place: its next piece could not be asked, or its ask
/// went with the pipe (lib.rs `Incoming::Down`, proto-reader N-1446 p251). He opens it again.
pub(crate) fn lost(s: &mut App, path: &std::path::Path) {
    let key = path_key(path);
    PIECES.lock().unwrap().remove(&key);
    s.images.insert(key, Img::Failed("disconnected before the picture came".into()));
}

/// Show image KEY (of `App::images`), fitted, adding it to the opened ones.
pub fn show(s: &mut App, key: String) {
    let o = &mut s.shown.opened;
    o.retain(|k| *k != key);
    o.push(key.clone());
    if o.len() > KEEP {
        let gone = o.remove(0);
        // a file's picture is read again if it is opened again; a chat's stays, as the chat shows it
        if gone.starts_with("path:") {
            s.images.remove(&gone);
        }
    }
    s.viewing = Some(key);
    fit(&mut s.shown);
}

fn fit(v: &mut Shown) {
    v.zoom = 1.0;
    v.at = None;
    v.wheel = 0.0;
}

/// The size of image KEY in pixels, when it is decoded.
fn size_of(s: &App, key: &str) -> Option<[f32; 2]> {
    match crate::chat::image(s, key) {
        Some(Img::Ready(img)) => img.size().map(|[w, h]| [w as f32, h as f32]),
        _ => None,
    }
}

/// The image area's size: the window less the room around it, the head and the strip.
fn area_size(v: &Shown) -> [f32; 2] {
    [v.area[2].max(1.0), v.area[3].max(1.0)]
}

/// Points per pixel when fitted: the whole image, or for a tall one the area's width; never above 1.
fn fitted(img: [f32; 2], area: [f32; 2]) -> f32 {
    let (w, h) = (area[0] / img[0], area[1] / img[1]);
    let tall = img[1] * w > area[1] * 1.5;
    (if tall { w } else { w.min(h) }).min(1.0)
}

/// The image's drawn size and top-left in the area, kept so that it covers the area where it is larger than it and is
/// centred where it is smaller.
fn place(v: &Shown, img: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let area = area_size(v);
    let scale = fitted(img, area) * v.zoom.max(0.01);
    let size = [img[0] * scale, img[1] * scale];
    let start = |i: usize| if size[i] > area[i] { if i == 1 { 0.0 } else { (area[i] - size[i]) / 2.0 } } else { (area[i] - size[i]) / 2.0 };
    let at = v.at.unwrap_or([start(0), start(1)]);
    let keep = |i: usize| if size[i] <= area[i] { (area[i] - size[i]) / 2.0 } else { at[i].clamp(area[i] - size[i], 0.0) };
    (size, [keep(0), keep(1)])
}

/// Zoom by FACTOR about the point P of the area (its own points): the pixel under P stays under it.
fn zoom_about(s: &mut App, factor: f32, p: [f32; 2]) {
    let Some(key) = s.viewing.clone() else { return };
    let Some(img) = size_of(s, &key) else { return };
    let v = &mut s.shown;
    let (_, at) = place(v, img);
    // out as far as the whole image (a tall one is first shown at the width), in as far as MAX_ZOOM
    let a = area_size(v);
    let whole = (a[0] / img[0]).min(a[1] / img[1]).min(1.0) / fitted(img, a);
    let to = (v.zoom * factor).clamp(whole.min(1.0), MAX_ZOOM);
    let k = to / v.zoom;
    v.zoom = to;
    v.at = Some([p[0] - (p[0] - at[0]) * k, p[1] - (p[1] - at[1]) * k]);
    let (_, kept) = place(v, img);
    v.at = Some(kept);
}

/// Move the image by D points.
fn shift(s: &mut App, d: [f32; 2]) {
    let Some(img) = s.viewing.as_ref().and_then(|k| size_of(s, k)) else { return };
    let v = &mut s.shown;
    let (_, at) = place(v, img);
    v.at = Some([at[0] + d[0], at[1] + d[1]]);
    let (_, kept) = place(v, img);
    v.at = Some(kept);
}

/// The opened image STEP places from the shown one (Left -1, Right +1), if there is one.
fn go(s: &mut App, step: isize) -> bool {
    let Some(cur) = s.viewing.clone() else { return false };
    let o = &s.shown.opened;
    let Some(i) = o.iter().position(|k| *k == cur) else { return false };
    let j = i as isize + step;
    if j < 0 || j as usize >= o.len() {
        return true;
    }
    s.viewing = Some(o[j as usize].clone());
    fit(&mut s.shown);
    true
}

/// A scene's zoom (`YAAIHH_IMAGE_ZOOM=F,X,Y`): the area is taken as the window less the viewer's room until it is laid
/// out (a still has one frame).
pub fn scene_zoom(s: &mut App, spec: Option<String>) {
    let (w, h) = std::env::var("YAAIHH_SIZE").ok().and_then(|z| { let (w, h) = z.split_once('x')?; Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)) }).unwrap_or((1280.0, 800.0));
    let strip = if s.shown.opened.len() > 1 { STRIP + 8.0 } else { 0.0 };
    s.shown.area = [PAD, PAD + HEAD + 8.0, w - 2.0 * PAD, h - 2.0 * PAD - HEAD - 8.0 - strip];
    let Some(spec) = spec else { return };
    let n: Vec<f32> = spec.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    if let [f, x, y] = n[..] {
        zoom_about(s, f, [x, y]);
    }
}

/// The viewer's keys while it is open; whether the key was one.
pub fn key(s: &mut App, ev: &KeyEvent) -> bool {
    if s.viewing.is_none() {
        return false;
    }
    // Alt+Left and Alt+Right are Back and Forward (history.rs `key`), not the image before and after
    if ev.mods.alt && matches!(ev.key, Key::Left | Key::Right) {
        return false;
    }
    let mid = { let a = area_size(&s.shown); [a[0] / 2.0, a[1] / 2.0] };
    match ev.key {
        Key::Escape => s.viewing = None,
        // on a PDF's page, the page before and after
        Key::Left | Key::Right if crate::files::viewing_pdf(s) => crate::files::turn_pdf(s, if ev.key == Key::Left { -1 } else { 1 }),
        Key::Left => return go(s, -1),
        Key::Right => return go(s, 1),
        Key::Command('=' | '+') => zoom_about(s, FACTOR, mid),
        Key::Command('-' | '_') => zoom_about(s, 1.0 / FACTOR, mid),
        Key::Command('0') => fit(&mut s.shown),
        Key::Up => shift(s, [0.0, 80.0]),
        Key::Down => shift(s, [0.0, -80.0]),
        Key::PageUp => shift(s, [0.0, area_size(&s.shown)[1] * 0.9]),
        Key::PageDown => shift(s, [0.0, -area_size(&s.shown)[1] * 0.9]),
        _ => return false,
    }
    true
}

/// The wheel while the viewer is open (ahead of the window's zoom): Ctrl zooms at the pointer, else it moves the image.
/// Whether it was taken (always, while open: nothing under the viewer scrolls).
pub fn wheel(s: &mut App, e: &WheelEvent) -> bool {
    if s.viewing.is_none() {
        return false;
    }
    if e.mods.ctrl {
        let a = s.shown.area;
        let p = [e.x - a[0], e.y - a[1]];
        s.shown.wheel += e.dy;
        let steps = (s.shown.wheel / NOTCH).trunc();
        if steps != 0.0 {
            s.shown.wheel -= steps * NOTCH;
            zoom_about(s, FACTOR.powf(steps), p);
        }
    } else {
        shift(s, [e.dx, e.dy]);
    }
    true
}

/// A short name for image KEY: a file's name, or whose chat it is from.
fn name_of(key: &str) -> String {
    // a file's picture: `path:PATH` here, `files|PATH` on the Files page
    if let Some(p) = key.strip_prefix("path:").or_else(|| key.strip_prefix("files|")) {
        return std::path::Path::new(p).file_name().map_or(p.to_string(), |n| n.to_string_lossy().into_owned());
    }
    match key.split('|').next() {
        Some(agent) if key.contains('|') => format!("{agent}'s image"),
        _ => key.to_string(),
    }
}

/// The viewer over the window below TOP, the bar's lower edge (the bar's arrows stay in reach), when an image is open.
pub fn layer(s: &App, k: &Kit, top: f32, layer: &mut Vec<rune::El>) {
    use rune::{col, row, text};
    let Some(key) = s.viewing.clone() else { return };
    let v = &s.shown;
    let o = &v.opened;
    let strip = o.len() > 1;
    let close = |s: &mut App| s.viewing = None;
    // the head: which image, how large, and the way out
    let pct = size_of(s, &key).map(|img| (fitted(img, area_size(v)) * v.zoom * 100.0).round() as i32);
    let place_in = o.iter().position(|x| *x == key).filter(|_| strip).map(|i| format!("{} of {} · ", i + 1, o.len())).unwrap_or_default();
    let head = row()
        .w_full()
        .h(HEAD)
        .gap(12.0)
        .center_y()
        .child(text(format!("{place_in}{}", name_of(&key))).font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(pct.map(|p| format!("{p}%")).unwrap_or_default()).font(k.ty.small).color(k.c.text_muted))
        .child(rune::el().grow(1.0))
        .child(text(if strip { "Ctrl+wheel zooms · drag moves · Ctrl 0 fits · ← → the others" } else { "Ctrl+wheel zooms · drag moves · Ctrl 0 fits" }).font(k.ty.small).color(k.c.text_faint))
        .child(text("×").font(k.ty.ui_medium).color(k.c.text_muted).cursor(rune::Cursor::Pointer).on_press(close));
    // the image, placed in a clipped area that takes the drag; a press that does not move, beside the image, closes
    let picture: rune::El = match crate::chat::image(s, &key) {
        Some(Img::Ready(img)) if img.error().is_none() => match img.size() {
            Some([w, h]) => {
                let (size, at) = place(v, [w as f32, h as f32]);
                rune::el().pin(Some(at[0]), Some(at[1]), None, None).w(size[0]).h(size[1]).pixels(img.pixels())
            }
            None => text("reading…").font(k.ty.small).color(k.c.text_faint),
        },
        Some(Img::Ready(img)) => text(format!("image: {}", img.error().map(|e| e.to_string()).unwrap_or_default())).font(k.ty.small).color(k.c.text_faint),
        Some(Img::Failed(why)) => text(format!("image: {why}")).font(k.ty.small).color(k.c.text_faint),
        Some(Img::Asked | Img::Bytes(_)) | None => text("image: reading…").font(k.ty.small).color(k.c.text_faint),
    };
    let area = rune::el()
        .key("image-viewer-area")
        .w_full()
        .grow(1.0)
        .shrink()
        .h(0.0)
        .clip_rounded(0.0)
        .cursor(rune::Cursor::Grab)
        .on_layout(|s: &mut App, r| s.shown.area = r)
        .on_drag(|s: &mut App, d: &mut rune::Drag, _tx: &mut Tx| match d.phase {
            rune::Phase::Down => {
                let img = s.viewing.as_ref().and_then(|k| size_of(s, k));
                s.shown.drag_from = img.map(|i| place(&s.shown, i).1).unwrap_or_default();
                s.shown.dragged = false;
            }
            rune::Phase::Move => {
                s.shown.dragged |= d.dx.abs() + d.dy.abs() > 4.0;
                let Some(img) = s.viewing.as_ref().and_then(|k| size_of(s, k)) else { return };
                let from = s.shown.drag_from;
                s.shown.at = Some([from[0] + d.dx, from[1] + d.dy]);
                let (_, kept) = place(&s.shown, img);
                s.shown.at = Some(kept);
            }
            rune::Phase::Up => {
                if d.cancelled() || s.shown.dragged {
                    return;
                }
                let Some(img) = s.viewing.as_ref().and_then(|k| size_of(s, k)) else { return };
                let (size, at) = place(&s.shown, img);
                let inside = d.x >= at[0] && d.x <= at[0] + size[0] && d.y >= at[1] && d.y <= at[1] + size[1];
                if !inside {
                    s.viewing = None;
                }
            }
        })
        .child(picture);
    let mut c = col().w_full().h_full().gap(8.0).child(head).child(area);
    if strip {
        let mut r = row().w_full().h(STRIP).gap(8.0).center_y();
        for x in o {
            let (open, here) = (x.clone(), *x == key);
            let thumb = match crate::chat::image(s, x) {
                Some(Img::Ready(img)) if img.error().is_none() => {
                    let [w, h] = img.size().map_or([1.0, 1.0], |[w, h]| [w as f32, h as f32]);
                    let tw = (w * (STRIP - 12.0) / h).clamp(24.0, 160.0);
                    rune::el().w(tw).h(STRIP - 12.0).pixels_fit(img.pixels(), rune::SurfaceFit::Fit)
                }
                _ => text(name_of(x)).font(k.ty.small).color(k.c.text_faint),
            };
            let border = if here { k.c.bg_accent } else { k.c.border_weak };
            r = r.child(rune::el().padding(4.0).radius(k.radius.sm).border(if here { 2.0 } else { 1.0 }, border).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| {
                s.viewing = Some(open.clone());
                fit(&mut s.shown);
            }).child(thumb));
        }
        c = c.child(r);
    }
    let viewer = rune::el().key("image-viewer").pin(Some(0.0), Some(top), Some(0.0), Some(0.0)).fill(k.c.bg_deep).padding(PAD).child(c);
    layer.push(crate::history::buttons(viewer));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(area: [f32; 2]) -> Shown {
        Shown { zoom: 1.0, area: [0.0, 0.0, area[0], area[1]], ..Default::default() }
    }

    #[test]
    fn a_tall_still_shows_at_the_window_width_from_its_top() {
        let v = shown([1200.0, 800.0]);
        let (size, at) = place(&v, [2560.0, 5800.0]);
        assert!((size[0] - 1200.0).abs() < 0.5);
        assert_eq!(at, [0.0, 0.0]);
    }

    #[test]
    fn a_wide_picture_fits_whole_and_centred() {
        let v = shown([1200.0, 800.0]);
        let (size, at) = place(&v, [2560.0, 1300.0]);
        assert!(size[0] <= 1200.0 && size[1] <= 800.0);
        assert!((at[1] - (800.0 - size[1]) / 2.0).abs() < 0.5);
    }

    #[test]
    fn a_small_picture_is_not_drawn_larger_than_its_pixels() {
        let v = shown([1200.0, 800.0]);
        assert_eq!(place(&v, [300.0, 200.0]).0, [300.0, 200.0]);
    }

    #[test]
    fn a_moved_image_never_leaves_a_gap() {
        let mut v = shown([1000.0, 800.0]);
        v.zoom = 3.0;
        v.at = Some([500.0, -99999.0]);
        let (size, at) = place(&v, [2000.0, 1000.0]);
        assert_eq!(at[0], 0.0);
        assert_eq!(at[1], 800.0 - size[1]);
    }

    #[test]
    fn the_strip_keeps_twelve() {
        assert_eq!(KEEP, 12);
    }

    #[test]
    fn images_are_found_by_their_ending() {
        assert!(is_image("/a/b/run2-thread.PNG"));
        assert!(is_image("x.jpeg"));
        assert!(!is_image("/a/b/notes.md"));
    }

    #[test]
    fn a_file_is_named_by_its_name_and_a_chat_image_by_its_agent() {
        assert_eq!(name_of("path:/a/b/run2-thread.png"), "run2-thread.png");
        assert_eq!(name_of("files|/a/b/c.jpg"), "c.jpg");
        assert_eq!(name_of("orch5|abc|120|0"), "orch5's image");
    }
}

#[cfg(test)]
mod remote_tests {
    use super::*;
    use base64::Engine;

    /// On Windows a picture named by its path comes from coordd in pieces, and is held whole once the last came; a
    /// refusal is the viewer's line in its place; the paths a chat names are found by coordd, and a picture it
    /// serves opens in the viewer.
    #[test]
    fn on_windows_a_picture_by_path_comes_from_coordd_in_pieces() {
        let (_held, mut s) = crate::view_cost::test_app();
        let png = crate::files::demo_png_bytes();
        let piece = |from: usize, to: usize| serde_json::json!({"size": png.len(), "modified": null, "data": base64::engine::general_purpose::STANDARD.encode(&png[from..to])}).to_string();
        let path = PathBuf::from("/home/t/Documents/papers/yaaihh-dev/still.png");
        let key = path_key(&path);
        let half = png.len() / 2;
        // the second piece is not asked here (no pipe): what came is kept, and the picture is not drawn half
        replied(&mut s, path.clone(), 0, true, piece(0, half));
        assert!(matches!(s.images.get(&key), Some(Img::Failed(e)) if e.contains("disconnected before")));
        replied(&mut s, path.clone(), 0, true, serde_json::json!({"size": png.len(), "data": base64::engine::general_purpose::STANDARD.encode(&png)}).to_string());
        assert!(matches!(s.images.get(&key), Some(Img::Bytes(b)) if b.len() == png.len()));
        replied(&mut s, path.clone(), 0, false, "coord w: never served: a .git part".into());
        assert!(matches!(s.images.get(&key), Some(Img::Failed(e)) if e.contains("never served")));
        // found by coordd, and served: the viewer opens it
        crate::paths::located(&mut s, String::new(), true, true, serde_json::json!({"path": path.display().to_string(), "readable": true}).to_string());
        assert_eq!(QUEUE.lock().unwrap().last(), Some(&path));
        QUEUE.lock().unwrap().clear();
        // not found: why, on the window's line
        crate::paths::located(&mut s, String::new(), true, false, "not found: x.png".into());
        assert_eq!(s.notice.as_deref(), Some("not found: x.png"));
        // a bridge older than the file requests: the one line, nothing asked
        crate::elsewhere::as_on_windows(|| {
            crate::paths::open("file:still.png", None);
            crate::paths::ask_located(&mut s);
        });
        assert!(s.notice.as_deref().is_some_and(|n| n.starts_with("still.png: ") && n.ends_with(crate::elsewhere::LINE)));
    }
}
