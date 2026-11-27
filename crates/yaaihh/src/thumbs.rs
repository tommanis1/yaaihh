//! A picture a post names, as a small picture in the post where it names it (the owner's walk item 16, yaaihh N-1.p184,
//! p188, p189 through yaaihh/fresh N-1724 p39: "it should show up a thumbnail I can click and it opens … not like this,
//! it's like it's any file, it's not any file"; "maybe just open in my Windows viewer"). A path coordd found
//! (paths.rs `linkify_found`) that names a picture (viewer.rs `is_image`) is drawn as Markdown's image, not as a link;
//! its bytes are read as the viewer reads a picture (on the disk here, through coordd's ReadFile on Windows); a click
//! opens the file in the desktop's own picture viewer (Windows' through explorer.exe on its `\\wsl.localhost` path),
//! never a popup of ours.
//!
//! Held on the window's thread: each picture by its full path, decoded through the window's cache once its bytes came,
//! at most [`KEEP`] of them (the oldest let go of first).

use crate::chat::Img;
use crate::App;
use std::cell::RefCell;
use std::collections::VecDeque;

/// The widest a picture in a post draws; a narrower one keeps its own width.
pub const THUMB_W: f32 = 280.0;
/// The most pictures held decoded for posts.
const KEEP: usize = 48;

enum Thumb {
    /// Its bytes asked (read on the next poll, or coordd's pieces coming).
    Asking,
    Ready(rune::Image),
    Failed,
}

thread_local! {
    /// The pictures by full path, the newest last.
    static HELD: RefCell<VecDeque<(String, Thumb)>> = const { RefCell::new(VecDeque::new()) };
    /// Full paths a drawn post named that are not asked yet, for the window's loop ([`poll`]).
    static TO_READ: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// TEXT as a post draws it (links already made, paths.rs): each link to a found picture left in its sentence as its
/// words alone (no link), and the picture drawn after the paragraph that names it, as Markdown's image whose alt text
/// is its path, so a sentence naming one mid-way stays whole (paper-research's note on H-1762).
pub fn as_pictures(text: &str) -> String {
    static LINK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(!?)\[([^\]\n]*)\]\((file:[^)\n]*)\)").unwrap());
    let mut out = String::with_capacity(text.len());
    for (i, para) in text.split("\n\n").enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        let mut pictures = Vec::new();
        let words = LINK.replace_all(para, |c: &regex::Captures| {
            if c[1].is_empty() && crate::viewer::is_image(crate::paths::without_line(&c[3])) {
                pictures.push(format!("![{}]({})", c[2].trim_matches('`'), &c[3]));
                c[2].to_string()
            } else {
                c[0].to_string()
            }
        });
        out.push_str(&words);
        for pic in pictures {
            out.push_str("\n\n");
            out.push_str(&pic);
        }
    }
    out
}

/// The pictures SRC names (its images' sources), each as it draws now: for a post's memo, so the post is drawn again
/// when one of them comes or decodes.
pub fn state(src: &str, full: impl Fn(&str) -> Option<String>) -> String {
    static IMAGE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"!\[[^\]\n]*\]\((file:[^)\n]*)\)").unwrap());
    let mut out = String::new();
    for c in IMAGE.captures_iter(src) {
        let mark = match full(&c[1]) {
            None => '-',
            Some(f) => HELD.with_borrow(|h| match h.iter().find(|(p, _)| *p == f).map(|(_, t)| t) {
                None | Some(Thumb::Asking) => 'a',
                Some(Thumb::Ready(i)) if i.is_ready() => 'r',
                Some(Thumb::Ready(_)) => 'd',
                Some(Thumb::Failed) => 'f',
            }),
        };
        out.push(mark);
    }
    out
}

/// The picture at full path FULL for a post: decoded once its bytes came; else none, and its bytes asked once.
pub fn picture(full: &str) -> Option<rune::Image> {
    let have = HELD.with_borrow(|h| h.iter().find(|(p, _)| p == full).map(|(_, t)| match t {
        Thumb::Ready(i) => Some(i.clone()),
        _ => None,
    }));
    match have {
        Some(i) => i,
        None => {
            HELD.with_borrow_mut(|h| {
                h.push_back((full.to_string(), Thumb::Asking));
                while h.len() > KEEP {
                    h.pop_front();
                }
            });
            TO_READ.with_borrow_mut(|t| t.push(full.to_string()));
            if let Some(w) = crate::viewer::WAKE.get() {
                w();
            }
            None
        }
    }
}

/// The window's loop: each picture a post named read (here) or asked of coordd (Windows, the viewer's ReadFile pieces),
/// and each whose bytes came decoded through the window's cache; whether any changed.
pub fn poll(s: &mut App) -> bool {
    for full in TO_READ.with_borrow_mut(std::mem::take) {
        crate::viewer::fetch(s, std::path::Path::new(&full));
    }
    let asking: Vec<String> = HELD.with_borrow(|h| h.iter().filter(|(_, t)| matches!(t, Thumb::Asking)).map(|(p, _)| p.clone()).collect());
    let mut changed = false;
    for full in asking {
        let key = crate::viewer::path_key(std::path::Path::new(&full));
        let now = match s.images.get(&key) {
            Some(Img::Bytes(b)) => match s.image_cache.as_ref() {
                Some(c) => Thumb::Ready(c.load(b.clone())),
                None => Thumb::Failed,
            },
            Some(Img::Ready(i)) => Thumb::Ready(i.clone()),
            Some(Img::Failed(_)) => Thumb::Failed,
            Some(Img::Asked) | None => continue,
        };
        // held here from now on; the viewer's copy goes unless the viewer shows it
        if !s.shown.opened.contains(&key) {
            s.images.remove(&key);
        }
        HELD.with_borrow_mut(|h| {
            if let Some(e) = h.iter_mut().find(|(p, _)| *p == full) {
                e.1 = now;
            }
        });
        changed = true;
    }
    changed
}

/// Open the picture at full path FULL in the desktop's own picture viewer: on Windows Windows' (explorer.exe, run on
/// the WSL machine with the file's `\\wsl.localhost` path, so Windows opens it with its picture app); on Linux the
/// desktop's handler.
pub fn open_outside(full: &str) {
    let cmd: Vec<String> = if crate::elsewhere::away() || cfg!(windows) {
        crate::places::server(vec!["sh".into(), "-c".into(), r#"exec explorer.exe "$(wslpath -w "$1")""#.into(), "sh".into(), full.into()])
    } else {
        vec!["xdg-open".into(), full.into()]
    };
    let _ = crate::places::command(&cmd[0]).args(&cmd[1..]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

#[cfg(test)]
pub(crate) fn clear() {
    HELD.with_borrow_mut(|h| h.clear());
    TO_READ.with_borrow_mut(|t| t.clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link to a picture: its words stay in the sentence, not a link, and the picture (its label as alt text, its
    /// backticks off) comes after the paragraph, each its own; a link to any other file and an image already stay.
    /// A thread whose one post, by an agent, says TEXT, the picture at FULL found by coordd (made up: no coordd).
    fn scene(text: &str, full: &str) -> (std::sync::MutexGuard<'static, ()>, App) {
        use serde_json::json;
        let (held, mut s) = crate::view_cost::test_app();
        s.image_cache = Some(rune::ImageCache::new(std::sync::Arc::new(|| {})));
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "yaaihh".into(), id: "thumbs".into(), boxed: false }, Vec::new());
        p.down = None;
        let v: coord_types::TicketView = serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "fresh"}, "title": "Home's repositories", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-25T08:00:00Z", "given": [],
            "posts": [{"n": 1, "at": "2026-11-25T08:00:00Z", "by": "owner", "text": "can you show me Home now?", "names": ["fresh"], "told": ["fresh"]},
                      {"n": 2, "at": "2026-11-25T08:02:00Z", "by": "fresh", "text": text, "names": [], "told": ["owner"]}]})).unwrap();
        p.live.tickets.insert(7, v);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(7);
        crate::paths::set_known("thumbs", full, None, crate::paths::Known::Found(full.to_string()));
        crate::paths::set_known("thumbs", "/nowhere/gone.png", None, crate::paths::Known::Missing);
        (held, s)
    }

    /// The still (STILL_OUT, made-up data, no coordd): a post naming a picture, YAAIHH_THUMB_PIC, drawn small where it
    /// names it: `STILL_OUT=DIR YAAIHH_THUMB_PIC=F yaaihh-tests thumbs::tests::still_picture_in_a_post --ignored --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn still_picture_in_a_post() {
        fn init() -> App {
            let full = std::env::var("YAAIHH_THUMB_PIC").unwrap();
            let text = format!("Here is Home as it is now: `{full}` — each repository's row opens it in the viewer, and Files is beside it.\n\nThe start folder is the first repository.");
            let (_held, mut s) = scene(&text, &full);
            // the stand-in's own folder has no projects.toml: none of that in a still
            s.notice = None;
            crate::paths::found("thumbs", &full, None);
            picture(&full);
            poll(&mut s);
            HELD.with_borrow(|h| h.iter().for_each(|(_, t)| if let Thumb::Ready(i) = t { let _ = i.wait(); }));
            s
        }
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let args = vec!["--snap".to_string(), out.join("picture-in-a-post.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    /// The owner's walk item 16: a post on Threads naming a picture coordd found draws the picture in place, at most
    /// [`THUMB_W`] wide, its bytes read once and decoded through the window's cache; no link of its path is drawn. One
    /// coordd did not find stays as written.
    #[test]
    fn a_post_naming_a_found_picture_draws_it_small_in_place() {
        clear();
        let d = std::env::temp_dir().join(format!("yaaihh-thumbs-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let pic = d.join("home-still.png");
        std::fs::write(&pic, crate::files::demo_png_bytes()).unwrap();
        let full = pic.display().to_string();
        let (_held, s) = scene(&format!("The still: `{full}`. The other: `/nowhere/gone.png`."), &full);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        // asked once, read on the poll, decoded
        assert!(rt.update_if(|s, _| poll(s)));
        let img = HELD.with_borrow(|h| h.iter().find(|(p, _)| *p == full).and_then(|(_, t)| match t { Thumb::Ready(i) => Some(i.clone()), _ => None }));
        assert_eq!(img.expect("decoded").wait().ok(), Some([96, 96]));
        assert!(!rt.state.images.contains_key(&crate::viewer::path_key(&pic)), "held here, not twice");
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let pics: Vec<[f32; 4]> = rt.elements().into_iter().filter(|e| !e.exiting && e.role == "image" && e.label.as_deref() == Some(full.as_str())).map(|e| e.rect).collect();
        assert_eq!(pics.len(), 1, "the picture drawn once, by its path as alt text");
        assert!(pics[0][2] > 0.0 && pics[0][2] <= THUMB_W, "small: {:?}", pics[0]);
        assert!(!rt.elements().into_iter().any(|e| e.role == "link" && e.label.as_deref().is_some_and(|l| l.contains("home-still.png"))), "no link of its path");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_found_pictures_link_becomes_its_picture() {
        // mid-sentence: the sentence whole, its words alone, the picture after the paragraph
        assert_eq!(as_pictures("see [`shot.png`](file:/w/shot.png) here, and more.\n\nNext."), "see `shot.png` here, and more.\n\n![shot.png](file:/w/shot.png)\n\nNext.");
        assert_eq!(as_pictures("[a.rs](file:/w/a.rs:3)"), "[a.rs](file:/w/a.rs:3)");
        assert_eq!(as_pictures("![x](file:/w/x.png)"), "![x](file:/w/x.png)");
        assert_eq!(as_pictures("[still](file:/w/s.JPG:2) and [b](file:/w/b.png)"), "still and b\n\n![still](file:/w/s.JPG:2)\n\n![b](file:/w/b.png)");
    }
}
