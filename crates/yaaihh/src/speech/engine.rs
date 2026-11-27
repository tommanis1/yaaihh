//! The engine behind `Recogniser` (N-554): transcribe.cpp's stream over the Parakeet model. The model is loaded once,
//! on the first take's thread, and kept for the run; each take is a stream of its own on a session of that model. Its
//! `committed` text only ever grows: its growth settles, its `tentative` text is the tail. The library's own log is
//! off, so no line of it holds a recognised word.

use super::{Recogniser, Said, Words, model};
use std::sync::{Mutex, OnceLock};
use transcribe_cpp::{Model, RunOptions, Session, Stream, StreamOptions};

/// The model is in (the box's "Loading speech's model…" goes).
pub static LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn loaded() -> Result<Model, String> {
    static M: OnceLock<Mutex<Option<Model>>> = OnceLock::new();
    let mut m = M.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if let Some(m) = &*m {
        return Ok(m.clone());
    }
    let path = model::present(&model::folder(), &model::PARAKEET).ok_or("Speech's model is not on this machine yet.")?;
    transcribe_cpp::disable_logging();
    let got = Model::load(&path).map_err(|e| format!("Speech's model could not be loaded: {e}."))?;
    *m = Some(got.clone());
    LOADED.store(true, std::sync::atomic::Ordering::Relaxed);
    super::wake();
    Ok(got)
}

/// A session of the loaded model, for one take.
pub fn session() -> Result<Session, String> {
    loaded()?.session().map_err(|e| format!("Speech could not start: {e}."))
}

/// One take's stream, borrowing the take's session.
pub struct Engine<'a> {
    stream: Stream<'a>,
    said: Said,
}

impl<'a> Engine<'a> {
    pub fn new(session: &'a mut Session) -> Result<Engine<'a>, String> {
        let stream = session.stream(&RunOptions::default(), &StreamOptions::default()).map_err(|e| format!("Speech could not start: {e}."))?;
        Ok(Engine { stream, said: Said::default() })
    }
}

impl Recogniser for Engine<'_> {
    fn feed(&mut self, pcm: &[f32]) -> Result<Words, String> {
        self.stream.feed(pcm).map_err(|e| format!("Speech stopped: {e}."))?;
        let t = self.stream.text();
        Ok(self.said.next(&t.committed, &t.tentative))
    }

    fn finish(&mut self) -> Result<Words, String> {
        self.stream.finalize().map_err(|e| format!("Speech stopped: {e}."))?;
        let t = self.stream.text();
        Ok(self.said.last(&t.committed, &t.tentative))
    }
}
