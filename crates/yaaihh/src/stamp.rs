//! Which version this window is, as he reads it at the bar's right end on every page: `0.4 · build 61` (the owner,
//! N-985 post 1: "version number show in my ui"; N-1054 post 1: "at the top right of my yaaihh in windows i want to see
//! the version number"; paper-research, N-898 posts 3 and 4). 0.4 is this crate's own `version` (its major and
//! minor; moving to the next minor is that one line of Cargo.toml and its yaaihh entry in Cargo.lock). Its patch digit is not
//! shown: it is the 0.4.x release the build was cut as, moved by docket-core in one commit on each tip (proto-lead,
//! N-1446 p862). The build is the train's number, stamped by the train into the exe as `YAAIHH_BUILD` (train-check
//! sets it from its train's folder, release.sh passes it on to cargo; agreed with docket-core, N-996). A build no
//! train made has no number and shows 0.4 alone. The release's long name (`YAAIHH_RELEASE`) is one click further.
//! Its words are the window's look's (`[words] version`, with `{version}` and `{build}`; coord_types::look): worded in
//! the flavour's look.toml, live with no build (consult-6, N-1104 p109; his p18).

use rune::El;
use rune_kit::Kit;

use crate::App;
use coord_types::look::Look;

/// What the window shows of its version: the stamped one, worded by LOOK (its `[words] version`).
pub fn shown(look: &Look) -> String {
    shown_of(look, env!("CARGO_PKG_VERSION"), option_env!("YAAIHH_BUILD"))
}

/// Version VERSION (`0.1.0`) and train number BUILD as LOOK words them (`[words] version`, its `{version}` and
/// `{build}`; the built-in `0.1 · build 17`), or `0.1` alone with no number.
pub fn shown_of(look: &Look, version: &str, build: Option<&str>) -> String {
    let short = version.split('.').take(2).collect::<Vec<_>>().join(".");
    match build.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => coord_types::look::say(look, "version", &[("version", &short), ("build", b)]),
        None => short,
    }
}

/// The release's long name, when the build was given one.
pub fn long() -> Option<&'static str> {
    option_env!("YAAIHH_RELEASE").filter(|r| !r.is_empty())
}

/// The bar's last item: the version in support level; a press shows the release's long name in its place,
/// and a second press the version again.
pub fn line(s: &App, k: &Kit) -> El {
    use rune_kit::{Button, ButtonVariant};
    let words = match long() {
        Some(name) if s.version_long => name.to_string(),
        _ => shown(crate::look::of(s).unwrap_or(&Look::default())),
    };
    let (small, muted) = (k.ty.small, k.c.text_muted);
    let shown_words = words.clone();
    Button::new(words)
        .variant(ButtonVariant::Ghost)
        .help(if s.version_long { "This window's version" } else { "This window's release" })
        .label_with(move |_, _| rune::text(shown_words.clone()).font(small).color(muted))
        .on_press_tx(|s: &mut App, _| s.version_long = !s.version_long)
        .build(k)
        .key("version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_reads_as_major_minor_and_the_trains_number() {
        let built = Look::default();
        assert_eq!(shown_of(&built, "0.1.0", Some("17")), "0.1 · build 17");
        assert_eq!(shown_of(&built, "0.2.3", Some(" 18\n")), "0.2 · build 18");
        // no train made it: the version alone
        assert_eq!(shown_of(&built, "0.1.0", None), "0.1");
        assert_eq!(shown_of(&built, "0.1.0", Some("")), "0.1");
        // worded by a look (N-1104 p113, head 4; his p18): no build for a new wording
        let look = coord_types::look::check(&serde_json::json!({"words": {"version": "build {build} of {version}"}})).unwrap();
        assert_eq!(shown_of(&look, "0.3.9", Some("69")), "build 69 of 0.3");
    }

    /// What the window shows is what this build was stamped with: the crate's version and `YAAIHH_BUILD` as cargo gave
    /// them to rustc, and 0.1 alone when the variable was not set.
    #[test]
    fn the_shown_version_is_the_stamped_one() {
        let short = env!("CARGO_PKG_VERSION").split('.').take(2).collect::<Vec<_>>().join(".");
        match option_env!("YAAIHH_BUILD").filter(|b| !b.trim().is_empty()) {
            Some(b) => assert_eq!(shown(&Look::default()), format!("{short} · build {}", b.trim())),
            None => assert_eq!(shown(&Look::default()), short),
        }
    }

    /// At the bar's right end, on every page: the stamped version; a press shows the long name (where the build has one)
    /// and a second press the version again.
    #[test]
    fn the_bar_shows_the_stamped_version_and_its_long_name_on_a_press() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        s.projects = vec![p];
        s.cur = 0;
        let mut rt = crate::view_cost::window(s);
        let texts = |rt: &rune::runtime::Runtime<App>| -> Vec<String> { rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect() };
        let frames = |rt: &mut rune::runtime::Runtime<App>| {
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
        };
        frames(&mut rt);
        assert!(texts(&rt).contains(&shown(&Look::default())), "{:?}", texts(&rt));
        let (x, y) = rt.find("version").expect("the version line is drawn");
        assert!(x > 1100.0 && y < 60.0, "at the top right of the 1400-wide window: {x}, {y}");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        frames(&mut rt);
        assert!(rt.state.version_long);
        if let Some(name) = long() {
            assert!(texts(&rt).iter().any(|t| t == name), "{:?}", texts(&rt));
        }
        let (x, y) = rt.find("version").expect("still drawn");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        frames(&mut rt);
        assert!(!rt.state.version_long && texts(&rt).contains(&shown(&Look::default())));
    }
}
