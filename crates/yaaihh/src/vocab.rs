//! The owner's vocabulary from the bar's `⋯`: a `vocab.md` beside projects.toml for every project, and one per project
//! (`projects/ID/vocab.md` there), each a few lines of Markdown he changes in the dialog itself.

use crate::App;
use rune::{El, ScrollSpec, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, TextArea};
use std::path::PathBuf;

/// The dialog's state (`App::vocab`): its files read when it opened (none: closed), the one being edited, what the
/// last Save said.
#[derive(Default)]
pub struct Vocab {
    files: Vec<Words>,
    editing: Option<(usize, TextEdit)>,
    said: Option<String>,
}

/// One file in the dialog: its key, its heading, where it is and its words.
struct Words {
    key: &'static str,
    heading: String,
    file: PathBuf,
    text: String,
}

/// The dialog opened: every project's file and the shown project's, read once here, each written first when new.
pub fn open(s: &mut App) {
    let mut files = vec![Words { key: "vocab-global", heading: "Every project".into(), text: text_of(&file(None), GLOBAL_START), file: file(None) }];
    if let Some(p) = s.proj() {
        let f = file(Some(&p.id));
        files.push(Words { key: "vocab-project", heading: p.name.clone(), text: text_of(&f, PROJECT_START), file: f });
    }
    s.vocab = Vocab { files, ..Vocab::default() };
}

/// What a new global `vocab.md` holds.
const GLOBAL_START: &str = "- **yaaihh**: this window and the engine behind it, which runs your agents and threads.\n- **tovrin**: one of your projects that yaaihh runs.\n";

/// What a new project's `vocab.md` holds.
const PROJECT_START: &str = "- **word**: what it means in this project, in one line.\n";

/// The global file, or project ID's.
pub fn file(id: Option<&str>) -> PathBuf {
    let cfg = PathBuf::from(crate::projects::config_path());
    let dir = cfg.parent().map(PathBuf::from).unwrap_or_default();
    match id {
        Some(id) => dir.join("projects").join(id).join("vocab.md"),
        None => dir.join("vocab.md"),
    }
}

/// FILE's text, written first with START when it is not there yet.
fn text_of(f: &PathBuf, start: &str) -> String {
    if let Ok(t) = std::fs::read_to_string(f) {
        return t;
    }
    let _ = f.parent().map(std::fs::create_dir_all);
    let _ = std::fs::write(f, start);
    start.to_string()
}

/// The words being edited written to their file and shown; when they could not be, kept in the field and why said.
fn save(s: &mut App) {
    let v = &mut s.vocab;
    let Some((i, t)) = v.editing.take() else { return };
    let Some(w) = v.files.get_mut(i) else { return };
    match std::fs::write(&w.file, &t.text) {
        Ok(()) => (w.text, v.said) = (t.text.clone(), None),
        Err(e) => (v.said, v.editing) = (Some(format!("{}: {e}", w.file.display())), Some((i, t))),
    }
}

/// File I's section: its heading and its words drawn with Edit, or, while edited, a field with Save and Cancel.
fn section(s: &App, i: usize, w: &Words, k: &Kit) -> El {
    let head = row().w_full().justify_between().center_y().child(text(w.heading.clone()).font(k.ty.small_medium).color(k.c.text_muted));
    let (key, c) = (w.key, col().key(w.key).w_full().gap(6.0));
    if let Some((_, t)) = s.vocab.editing.as_ref().filter(|(e, _)| *e == i) {
        let area = TextArea::new(t).width(None).auto_grow(4, 14).on_edit(|s: &mut App| &mut s.vocab.editing.as_mut().unwrap().1).build(k);
        let save = Button::new("Save").on_press(save).build(k).key(format!("{key}-save"));
        let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.vocab.editing = None).build(k);
        return c.child(head).child(area).child(row().w_full().gap(8.0).justify_end().child(cancel).child(save));
    }
    let edit = Button::new("Edit").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        let now = s.vocab.files.get(i).map(|w| TextEdit::new(&w.text));
        (s.vocab.editing, s.vocab.said) = (now.map(|t| (i, t)), None);
    });
    c.child(head.child(edit.build(k).key(format!("{key}-edit")))).child(rune_kit::markdown::Markdown::new(w.text.clone()).build(k))
}

/// The Vocabulary dialog into LAYER while it is open: every project's words, then the shown project's.
pub fn dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    if s.vocab.files.is_empty() {
        return;
    }
    let mut list = col().w_full().gap(16.0).children(s.vocab.files.iter().enumerate().map(|(i, w)| section(s, i, w, k)));
    if let Some(e) = &s.vocab.said {
        list = list.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap().key("vocab-said"));
    }
    // the spelling's own words, kept one press away
    let added = Button::new(format!("Words you added ({})", s.spell.added().len())).variant(ButtonVariant::Ghost).on_press(|s: &mut App| {
        s.vocab = Vocab::default();
        s.spell.words_open = true;
    });
    let close = Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.vocab = Vocab::default()).build(k);
    let body = col()
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .child(col().w_full().max_h(420.0).scroll(ScrollSpec { stick_bottom: false }).child(list))
        .child(row().w_full().justify_between().child(added.build(k).key("vocab-added")).child(close));
    Dialog::new("vocab", "Vocabulary").open(true).fit().width(460.0).child(body).on_close(|s: &mut App| s.vocab = Vocab::default()).build(k, layer);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bar's `⋯` is lean: no zoom rows at 100%, no words row; its Vocabulary opens every project's words and the
    /// project's, each file written on first sight and changed in the dialog.
    #[test]
    fn the_bar_menu_is_lean_and_its_vocabulary_shows_both_files() {
        let (_held, s) = crate::home::tests::made();
        let id = s.proj().map(|p| p.id.clone()).unwrap();
        let (g, p) = (file(None), file(Some(&id)));
        let _ = (std::fs::remove_file(&g), std::fs::remove_file(&p));
        let mut rt = crate::view_cost::window(s);
        rt.update(|s, _| s.bar_menu = true);
        crate::home::tests::settle(&mut rt);
        let t = crate::home::tests::texts(&rt);
        for gone in ["Zoom in", "Zoom out", "Words you added (0)…"] {
            assert!(!t.iter().any(|x| x.starts_with(gone)), "{gone}: {t:?}");
        }
        assert!(t.iter().any(|x| x == "Vocabulary") && t.iter().any(|x| x == "Dark mode"), "{t:?}");
        rt.update(|s, _| {
            s.bar_menu = false;
            open(s);
        });
        crate::home::tests::settle(&mut rt);
        let t = crate::home::tests::texts(&rt);
        assert!(t.iter().any(|x| x == "Every project") && t.iter().any(|x| x.contains("this window and the engine")), "{t:?}");
        assert_eq!((std::fs::read_to_string(&g).unwrap(), std::fs::read_to_string(&p).unwrap()), (GLOBAL_START.to_string(), PROJECT_START.to_string()));
        // Edit opens a field in the dialog itself, Save writes the file and draws it again
        crate::home::tests::keyed(&mut rt, "vocab-global-edit");
        std::fs::write(&g, "changed outside").unwrap();
        rt.update(|s, _| s.vocab.editing.as_mut().unwrap().1 = TextEdit::new("- **docket**: the language of his programs.\n"));
        crate::home::tests::keyed(&mut rt, "vocab-global-save");
        assert_eq!(std::fs::read_to_string(&g).unwrap(), "- **docket**: the language of his programs.\n");
        assert!(rt.state.vocab.editing.is_none() && crate::home::tests::texts(&rt).iter().any(|x| x.contains("the language of his programs")));
        // drawn from what the dialog read: a frame reads no file
        std::fs::remove_file(&g).unwrap();
        crate::home::tests::settle(&mut rt);
        assert!(!g.exists() && crate::home::tests::texts(&rt).iter().any(|x| x.contains("the language of his programs")));
        // zoomed, the way back to 100% is the one zoom row
        rt.update(|s, tx| {
            (s.vocab, s.bar_menu) = (Vocab::default(), true);
            s.zoom.restore(1.3);
            s.zoom.show(tx);
        });
        crate::home::tests::settle(&mut rt);
        assert!(crate::home::tests::texts(&rt).iter().any(|x| x == "Actual size (now 130%)"));
    }

    /// The stills of the lean `⋯`s (the bar's, an agent's) and the Vocabulary: `STILL_OUT=DIR cargo test vocab_stills -- --ignored`.
    #[test]
    #[ignore]
    fn vocab_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn menu() -> App {
            let (_held, mut s) = crate::home::tests::made();
            (s.bar_menu, s.notice) = (true, None);
            s
        }
        fn words() -> App {
            let (_held, mut s) = crate::home::tests::made();
            s.notice = None;
            open(&mut s);
            s
        }
        fn editing() -> App {
            let mut s = words();
            let now = TextEdit::new(&s.vocab.files[0].text);
            s.vocab.editing = Some((0, now));
            s
        }
        fn agent_menu() -> App {
            let (_held, mut s) = crate::home::tests::made();
            s.sel = crate::Sel::Agent("try-1".into());
            s.notice = None;
            s.streams.more.insert((0, "try-1".into()));
            s
        }
        let shots: [(&str, fn() -> App); 4] = [("bar-menu.png", menu), ("vocabulary.png", words), ("vocabulary-edit.png", editing), ("agent-menu.png", agent_menu)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
