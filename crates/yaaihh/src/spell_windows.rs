//! The composer's spelling on Windows: Windows' own checker (`ISpellChecker`), for the languages added in Settings ›
//! Time & language, the first of the user's display languages that has a dictionary, else US English. One checker
//! per thread that checks (the window's), made on first use and kept: releasing it as the thread ends can wait on
//! its provider (as Rune's own `spell/windows.rs` notes).

use windows::Win32::Globalization::{
    CORRECTIVE_ACTION_GET_SUGGESTIONS, CORRECTIVE_ACTION_REPLACE, GetUserPreferredUILanguages, ISpellChecker, ISpellCheckerFactory,
    ISpellingError, MUI_LANGUAGE_NAME, SpellCheckerFactory,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, IEnumString};
use windows::core::{HSTRING, PWSTR};

thread_local! {
    static CHECKER: std::mem::ManuallyDrop<Option<ISpellChecker>> = std::mem::ManuallyDrop::new(make());
}

fn make() -> Option<ISpellChecker> {
    // SAFETY: COM calls on this thread's apartment; the window's thread has one already, a test's may not.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let factory: ISpellCheckerFactory = CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        preferred().into_iter().chain(["en-US".to_string()]).find_map(|tag| {
            let tag = HSTRING::from(tag);
            factory.IsSupported(&tag).ok().filter(|b| b.as_bool())?;
            factory.CreateSpellChecker(&tag).ok()
        })
    }
}

/// The user's display languages, most preferred first.
fn preferred() -> Vec<String> {
    let (mut count, mut len) = (0u32, 0u32);
    // SAFETY: the first call sizes the buffer the second fills; the buffer outlives both.
    unsafe {
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, None, &mut len).is_err() || len == 0 {
            return Vec::new();
        }
        let mut buffer = vec![0u16; len as usize];
        if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, Some(PWSTR(buffer.as_mut_ptr())), &mut len).is_err() {
            return Vec::new();
        }
        buffer.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
    }
}

/// The first English among his display languages (`en-GB`, …), whether or not it is the checker's first language.
pub(super) fn english() -> Option<String> {
    preferred().into_iter().find(|t| t.to_lowercase().starts_with("en"))
}

pub(super) fn available() -> bool {
    CHECKER.with(|c| c.is_some())
}

/// Whether W is a word the checker knows (a repeated word, "the the", is not the checker's to mark here).
pub(super) fn check(w: &str) -> bool {
    CHECKER.with(|c| {
        let Some(c) = &**c else { return true };
        // SAFETY: the checker is this thread's, and the word outlives the call.
        unsafe {
            let Ok(errors) = c.Check(&HSTRING::from(w)) else { return true };
            loop {
                let mut error: Option<ISpellingError> = None;
                if errors.Next(&mut error).is_err() {
                    return true;
                }
                let Some(error) = error else { return true };
                let action = error.CorrectiveAction().unwrap_or_default();
                if action == CORRECTIVE_ACTION_GET_SUGGESTIONS || action == CORRECTIVE_ACTION_REPLACE {
                    return false;
                }
            }
        }
    })
}

/// What the checker offers for W, best first.
pub(super) fn suggest(w: &str) -> Vec<String> {
    CHECKER.with(|c| {
        let Some(c) = &**c else { return Vec::new() };
        // SAFETY: as in `check`; the list is read to its end on this thread.
        unsafe { c.Suggest(&HSTRING::from(w)).map(|list| strings(&list)).unwrap_or_default() }
    })
}

/// Reads a COM string list to its end.
///
/// # Safety
/// `list` must be a live enumerator on this thread.
unsafe fn strings(list: &IEnumString) -> Vec<String> {
    let mut found = Vec::new();
    loop {
        let mut one = [PWSTR::null()];
        let mut fetched = 0u32;
        // SAFETY: per this function's contract; each string handed over is freed once read.
        unsafe {
            if list.Next(&mut one, Some(&mut fetched)).is_err() || fetched == 0 || one[0].is_null() {
                break;
            }
            found.extend(one[0].to_string().ok());
            CoTaskMemFree(Some(one[0].0 as *const _));
        }
    }
    found
}
