//! The texts the engine loaded, each set kept beside the record under its `loaded` line's hash (spec 11).

use docket_v2::check::names::Loaded;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where the texts loaded under `hash` are kept: `RECORD.texts/HASH.json`.
pub fn place(record: &Path, hash: &str) -> PathBuf {
    let mut dir = record.as_os_str().to_owned();
    dir.push(".texts");
    PathBuf::from(dir).join(format!("{hash}.json"))
}

/// The texts `texts`, each file's name and text, read and loaded together.
pub fn load(texts: &[(String, String)]) -> Result<Loaded, String> {
    let modules = texts.iter().map(|(p, t)| docket_v2::syntax::read(t).map_err(|e| format!("{p}: {e}"))).collect::<Result<_, _>>()?;
    docket_v2::check::load(modules).map_err(|e| e.to_string())
}

/// The texts kept beside `record` under `hash`, loaded; an error if none are kept there or they load to another hash.
pub fn kept(record: &Path, hash: &[u8; 32]) -> Result<Loaded, String> {
    let at = place(record, &crate::codec::hex(hash));
    let texts: Vec<(String, String)> = serde_json::from_slice(&std::fs::read(&at).map_err(|e| format!("{}: {e}", at.display()))?).map_err(|e| format!("{}: {e}", at.display()))?;
    let loaded = load(&texts)?;
    (loaded.hash == *hash).then_some(loaded).ok_or(format!("{} loads to another hash", at.display()))
}

/// Write `bytes` to `to` whole and on disk before it is renamed into place, so it is never part of a file.
fn write_whole(to: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(to.parent().unwrap_or(Path::new(".")))?;
    let new = to.with_extension("new");
    let mut f = std::fs::File::create(&new)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&new, to)?;
    // the rename itself on disk too
    std::fs::File::open(to.parent().unwrap_or(Path::new(".")))?.sync_all()
}

/// Keep `texts`, each file's name and text in the order loaded, under `hash`, before a `loaded` line names it. The
/// file is written whole and on disk before it is renamed into place, so a kept file is never part of one; a hash
/// kept already is left as it is, its texts being the same.
pub fn keep(record: &Path, hash: &str, texts: &[(String, String)]) -> std::io::Result<()> {
    let to = place(record, hash);
    if to.exists() {
        return Ok(());
    }
    write_whole(&to, &serde_json::to_vec(texts)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_set_of_texts_is_kept_once_under_its_hash() {
        let dir = std::env::temp_dir().join(format!("texts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("record.jsonl");
        let texts = [("a.docket".to_string(), "module a".to_string()), ("b.docket".into(), "module b".into())];
        keep(&record, "ab12", &texts).unwrap();
        let at = dir.join("record.jsonl.texts/ab12.json");
        assert_eq!(place(&record, "ab12"), at);
        let read: Vec<(String, String)> = serde_json::from_slice(&std::fs::read(&at).unwrap()).unwrap();
        assert_eq!(read, texts, "in the order loaded");
        keep(&record, "ab12", &[]).unwrap();
        assert_eq!(serde_json::from_slice::<Vec<(String, String)>>(&std::fs::read(&at).unwrap()).unwrap(), texts, "kept once");
        assert!(!at.with_extension("new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
