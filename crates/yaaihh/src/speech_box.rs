//! Speech in the message box (N-554; paper-research N-554.p14, docket-edge2's design N-554.p5): the mic beside Send,
//! Ctrl+Shift+Space the same, accent while a take runs. A take's settled words go in at the caret as plain text, its
//! tail is drawn as a composition. The Esc key does nothing to a take (his rule: Esc only interrupts, and the Esc
//! button beside the mic stops the agent); Ctrl+Z takes a whole take back as one step (paper-research N-585.p11). With
//! no model on this machine, a press offers it on a line under the box; his click on Download is the yes. That line
//! goes to the box he pressed in last (the minder, consult-4, N-585.p10).

use crate::App;
use crate::speech::{self, Heard, model};
use rune::{El, Ime, TextEdit, row, text};
use rune_kit::{Button, ButtonVariant, IconButton, IconButtonVariant, Kit};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

type Key = (usize, String);

#[derive(Default)]
pub struct SpeechBox {
    take: Option<Take>,
    /// the line under one box: the offer of the model, its fetch, or why there was no take
    line: Option<(Key, Line)>,
    fetch: Option<(Arc<model::Progress>, Receiver<Result<PathBuf, String>>)>,
    /// A box he pressed in while a take of another box was still running: its take starts once that one has ended,
    /// so the first one's last words still go into its own box.
    next: Option<Key>,
    /// "Loading speech's model…" as last drawn, so its going is drawn
    loading: bool,
}

struct Take {
    key: Key,
    id: u64,
    /// stopped by his press: its last words still come
    stopping: bool,
    /// where its settled words went, each (start, "", words), followed through his own edits as spell.rs follows its
    /// corrections; the box's text as last seen
    put: Vec<(usize, String, String)>,
    seen: String,
    /// he edited the box while it ran: its pieces then stay one undo step each
    his: bool,
}

enum Line {
    Offer,
    Fetching,
    Said(String),
}

fn take_key(key: &Key) -> String {
    format!("{}/{}", key.0, key.1)
}

/// Whether a take of box KEY runs (the mic in accent).
pub fn open(s: &App, key: &Key) -> bool {
    s.speech_box.take.as_ref().is_some_and(|t| t.key == *key && !t.stopping)
}

/// The mic's press, or Ctrl+Shift+Space in the box: a take starts, or the one running stops.
pub fn press(s: &mut App, key: &Key) {
    let sb = &mut s.speech_box;
    if let Some(t) = &mut sb.take {
        if !t.stopping {
            t.stopping = true;
            speech::stop();
        }
        // another box: its take starts when this one's last words are in
        if t.key != *key {
            sb.next = Some(key.clone());
        }
        return;
    }
    if sb.fetch.is_some() {
        // the fetch's line comes to the box he is in
        if let Some((at, _)) = &mut sb.line {
            *at = key.clone();
        }
        return;
    }
    if speech::needs_model() {
        // a processor the engine cannot run on is said before anything is offered or fetched
        let line = if speech::processor_ok() { Line::Offer } else { Line::Said(format!("Speech needs {}.", speech::FLOOR)) };
        sb.line = Some((key.clone(), line));
        return;
    }
    sb.line = None;
    let project = s.projects.get(key.0).map_or(String::new(), |p| p.id.clone());
    match speech::start(&take_key(key), &project) {
        Ok(id) => {
            let seen = s.composers.entry(key.clone()).or_insert_with(|| TextEdit::new("")).text.clone();
            s.speech_box.take = Some(Take { key: key.clone(), id, stopping: false, put: Vec::new(), seen, his: false });
        }
        Err(why) => s.speech_box.line = Some((key.clone(), Line::Said(why))),
    }
}

/// At a take's end: its pieces, one undo step each as they went in, made one step, so Ctrl+Z takes the whole take
/// back. Only when he did not edit the box meanwhile (then each undo is one of the take's pieces); a history that is not
/// as expected is put back as it was. PUT in the order the pieces went in. True when made one.
fn one_step(e: &mut TextEdit, put: &[(usize, String, String)]) -> bool {
    if put.len() < 2 {
        return false;
    }
    let end = e.text.clone();
    let mut start = end.clone();
    for (at, _, w) in put.iter().rev() {
        start.replace_range(*at..at + w.len(), "");
    }
    for _ in 0..put.len() {
        e.undo();
    }
    if e.text != start {
        for _ in 0..put.len() {
            e.redo();
        }
        return false;
    }
    e.undo_group(|e| {
        for (at, _, w) in put {
            e.set_caret(*at);
            e.insert(w);
        }
    });
    e.text == end
}

/// The box's own key for speech: Ctrl+Shift+Space. True when taken.
pub fn key(s: &mut App, key: &Key, ev: &rune::KeyEvent) -> bool {
    let space = matches!(&ev.key, rune::Key::Text(t) if t == " ") || matches!(ev.key, rune::Key::Command(' '));
    if space && ev.mods.ctrl && ev.mods.shift {
        press(s, key);
        return true;
    }
    false
}

/// After each edit of box KEY (chat.rs `on_change`): the take's words followed through it.
pub fn changed(s: &mut App, key: &Key) {
    let Some(t) = s.speech_box.take.as_mut().filter(|t| t.key == *key) else { return };
    if let Some(e) = s.composers.get(key) {
        t.his |= e.text != t.seen;
        crate::spell::follow(&t.seen, &e.text, &mut t.put);
        t.seen = e.text.clone();
    }
}

/// Settled WORDS at the caret of E: a space before them when they follow a word, none at the box's start. Where they
/// went.
fn put_in(e: &mut TextEdit, words: &str) -> Option<(usize, String)> {
    if words.trim().is_empty() {
        return None;
    }
    let before = e.text[..e.anchor.min(e.focus).min(e.text.len())].chars().next_back();
    let words = match before {
        None => words.trim_start().to_string(),
        Some(c) if !c.is_whitespace() && !words.starts_with(char::is_whitespace) && !words.starts_with([',', '.', '?', '!', ';', ':']) => format!(" {words}"),
        _ => words.to_string(),
    };
    e.ime(&Ime::Preedit(String::new(), None));
    let at = e.anchor.min(e.focus);
    e.insert(&words);
    Some((at, words))
}

/// What came from takes and from the fetch, each frame the waker wakes. True when a box changed.
pub fn poll(s: &mut App, tx: &mut rune::Tx) -> bool {
    let mut changed = false;
    let loading = s.speech_box.take.is_some() && speech::model_loading();
    if loading != s.speech_box.loading {
        s.speech_box.loading = loading;
        changed = true;
    }
    for (_, id, h) in speech::poll() {
        let Some(t) = s.speech_box.take.as_mut().filter(|t| t.id == id) else { continue };
        let key = t.key.clone();
        let e = s.composers.entry(key.clone()).or_insert_with(|| TextEdit::new(""));
        crate::spell::follow(&t.seen, &e.text, &mut t.put);
        match h {
            Heard::Tail(tail) => {
                e.ime(&Ime::Preedit(tail, None));
            }
            Heard::Settled(words) => {
                if let Some((at, w)) = put_in(e, &words) {
                    t.put.push((at, String::new(), w));
                }
            }
            Heard::Failed(why) => s.speech_box.line = Some((key.clone(), Line::Said(why))),
            Heard::End => {
                e.ime(&Ime::Preedit(String::new(), None));
                if !t.his {
                    one_step(e, &t.put);
                }
                s.speech_box.take = None;
                if let Some(next) = s.speech_box.next.take() {
                    press(s, &next);
                }
            }
        }
        if let Some(t) = s.speech_box.take.as_mut().filter(|t| t.key == key)
            && let Some(e) = s.composers.get(&key)
        {
            t.seen = e.text.clone();
        }
        changed = true;
    }
    if let Some(came) = s.speech_box.fetch.as_ref().map(|(_, done)| done.try_recv()) {
        match came {
            Ok(got) => {
                let said = match got {
                    Ok(_) => "Press the mic to speak.".to_string(),
                    Err(why) => why,
                };
                s.speech_box.fetch = None;
                if let Some((_, line)) = &mut s.speech_box.line {
                    *line = Line::Said(said);
                }
                changed = true;
            }
            Err(_) => {
                // its progress, drawn twice a second
                s.wake_every(Duration::from_millis(500), tx);
                changed = true;
            }
        }
    }
    changed
}

/// His Download: the fetch starts on a thread of its own (model.rs: the pinned address, its hash checked).
fn download(s: &mut App) {
    let progress = Arc::new(model::Progress::default());
    let (send, done) = channel();
    let p = progress.clone();
    let started = std::thread::Builder::new().name("speech-model".into()).spawn(move || {
        let dir = model::folder();
        let got = std::fs::create_dir_all(&dir).map_err(|e| format!("Speech's model has no folder at {}: {e}.", dir.display())).and_then(|_| model::fetch(&dir, &model::PARAKEET, &p));
        let _ = send.send(got);
        speech::wake();
    });
    match started {
        Ok(_) => {
            s.speech_box.fetch = Some((progress, done));
            if let Some((_, line)) = &mut s.speech_box.line {
                *line = Line::Fetching;
            }
        }
        Err(e) => {
            if let Some((_, line)) = &mut s.speech_box.line {
                *line = Line::Said(format!("Speech's model could not be fetched: {e}."));
            }
        }
    }
}

/// The mic beside Send, in a build that takes speech: accent while a take of this box runs.
pub fn button(s: &App, key: &Key, k: &Kit) -> Option<El> {
    if !speech::shown() {
        return None;
    }
    let on = open(s, key);
    // listening in another box (he left it): every mic shows it, and a press here stops it and listens here
    let elsewhere = !on && s.speech_box.take.as_ref().is_some_and(|t| !t.stopping);
    let k2 = key.clone();
    IconButton::new(&rune_kit::glyphs::MIC)
        .variant(if on || elsewhere { IconButtonVariant::Contrast } else { IconButtonVariant::Ghost })
        .label(if on { "Stop speaking" } else { "Speak" })
        .help(if on {
            "Stop listening (Ctrl+Shift+Space)"
        } else if elsewhere {
            "Speak here instead (Ctrl+Shift+Space)"
        } else {
            "Speak (Ctrl+Shift+Space)"
        })
        .on_press(move |s: &mut App| press(s, &k2))
        .build(k)
        .into()
}

/// The line under box KEY, where the candidates row is, when speech has something to say there.
pub fn line(s: &App, key: &Key, k: &Kit) -> Option<El> {
    // the first take of a run, while the model loads: what he says waits for it and is not lost
    if s.speech_box.take.as_ref().is_some_and(|t| t.key == *key) && speech::model_loading() {
        return Some(row().child(text("Loading speech's model…").font(k.ty.small).color(k.c.text_muted)));
    }
    let (at, l) = s.speech_box.line.as_ref()?;
    if at != key {
        return None;
    }
    let m = &model::PARAKEET;
    let dismiss = |label: &str| Button::new(label).variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.speech_box.line = None).build(k);
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).pr(4.0);
    Some(match l {
        Line::Offer => row()
            .child(small(format!("Download speech model ({} MB)", m.bytes / 1_000_000)))
            .child(Button::new("Download").variant(ButtonVariant::Ghost).on_press(download).build(k))
            .child(dismiss("Not now")),
        Line::Fetching => {
            let got = s.speech_box.fetch.as_ref().map_or(0, |(p, _)| p.got.load(Ordering::Relaxed));
            row().child(small(format!("Fetching speech's model: {} of {} MB", got / 1_000_000, m.bytes / 1_000_000))).child(
                Button::new("Stop")
                    .variant(ButtonVariant::Ghost)
                    .on_press(|s: &mut App| {
                        if let Some((p, _)) = &s.speech_box.fetch {
                            p.stop.store(true, Ordering::Relaxed);
                        }
                    })
                    .build(k),
            )
        }
        Line::Said(why) => row().child(small(why.clone())).child(dismiss("OK")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settled words go in at the caret with one space between them and a word before, none at the start.
    #[test]
    fn settled_words_go_in_at_the_caret_spaced() {
        let mut e = TextEdit::new("");
        assert_eq!(put_in(&mut e, " Hello"), Some((0, "Hello".into())));
        assert_eq!(put_in(&mut e, "world,"), Some((5, " world,".into())));
        assert_eq!(put_in(&mut e, " how"), Some((12, " how".into())));
        assert_eq!(e.text, "Hello world, how");
        e.set_caret(5);
        assert_eq!(put_in(&mut e, ", there"), Some((5, ", there".into())));
        assert_eq!(e.text, "Hello, there world, how");
        assert_eq!(put_in(&mut e, "   "), None);
    }

    /// A take's pieces become one undo step at its end: one Ctrl+Z takes the whole take back, his text before it stays.
    #[test]
    fn a_whole_take_is_one_undo_step() {
        let mut e = TextEdit::new("Note:");
        e.set_caret(5);
        let mut put = Vec::new();
        for w in ["Hello", "world,", " how"] {
            let (at, w) = put_in(&mut e, w).unwrap();
            put.push((at, String::new(), w));
        }
        assert_eq!(e.text, "Note: Hello world, how");
        assert!(one_step(&mut e, &put));
        assert_eq!(e.text, "Note: Hello world, how");
        e.undo();
        assert_eq!(e.text, "Note:");
        e.redo();
        assert_eq!(e.text, "Note: Hello world, how");
        // a history that is not the take's (he typed in it): left as it was
        let mut f = TextEdit::new("");
        let (at, w) = put_in(&mut f, "One").unwrap();
        f.insert(" mine");
        let (at2, w2) = put_in(&mut f, "two").unwrap();
        assert!(!one_step(&mut f, &[(at, String::new(), w), (at2, String::new(), w2)]));
        assert_eq!(f.text, "One mine two");
    }
}
