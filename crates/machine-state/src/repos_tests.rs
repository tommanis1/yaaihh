use super::top_pdf;
use std::time::{Duration, SystemTime};

/// A fresh folder for test NAME.
fn folder(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("machine-state-pdf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A PDF at D/REL made AGO seconds back.
fn pdf(d: &std::path::Path, rel: &str, ago: u64) {
    let p = d.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "%PDF").unwrap();
    std::fs::File::options().write(true).open(&p).unwrap().set_modified(SystemTime::now() - Duration::from_secs(ago)).unwrap();
}

/// The owner's N-2003 (3): a built PDF is found in `.build/`, `build/`, `out/` or the top; `main.pdf` or `paper.pdf`
/// first, else the newest.
#[test]
fn the_built_pdf_is_found_in_its_build_folders_main_or_paper_first_else_the_newest() {
    let d = folder("found");
    let at = |d: &std::path::Path| top_pdf(&d.display().to_string());
    assert_eq!(at(&d), None);
    pdf(&d, ".build/draft.pdf", 50);
    assert_eq!(at(&d).as_deref(), Some(".build/draft.pdf"), "in .build");
    pdf(&d, "out/slides.pdf", 10);
    assert_eq!(at(&d).as_deref(), Some("out/slides.pdf"), "the newest");
    pdf(&d, "build/paper.pdf", 900);
    assert_eq!(at(&d).as_deref(), Some("build/paper.pdf"), "paper.pdf before a newer one");
    pdf(&d, "notes.pdf", 1);
    pdf(&d, "main.pdf", 2000);
    pdf(&d, ".build/main.pdf", 5);
    assert_eq!(at(&d).as_deref(), Some(".build/main.pdf"), "of the named, the newest");
    pdf(&d, "deep/x/main.pdf", 0);
    pdf(&d, "build/sub/main.pdf", 0);
    assert_eq!(at(&d).as_deref(), Some(".build/main.pdf"), "nothing deeper");
    let _ = std::fs::remove_dir_all(&d);
}

/// A link is never followed: a linked PDF, or a build folder that is a link, is not the repository's.
#[test]
fn a_linked_pdf_or_build_folder_is_not_followed() {
    let d = folder("links");
    let away = folder("away");
    pdf(&away, "main.pdf", 0);
    pdf(&away, "inner/paper.pdf", 0);
    std::os::unix::fs::symlink(away.join("main.pdf"), d.join("main.pdf")).unwrap();
    std::os::unix::fs::symlink(away.join("inner"), d.join("out")).unwrap();
    std::os::unix::fs::symlink(&away, d.join(".build")).unwrap();
    assert_eq!(top_pdf(&d.display().to_string()), None);
    pdf(&d, "build/old.pdf", 100);
    assert_eq!(top_pdf(&d.display().to_string()).as_deref(), Some("build/old.pdf"), "only its own");
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&away);
}
