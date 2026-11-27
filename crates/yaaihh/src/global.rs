//! The window's own Home, above every project: the projects, the storage of all of them together and the machines they
//! run on, and the way to the settings of every project. Each section shows its heading from the first frame.

use crate::{App, Proj};
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::Kit;

/// The page's view id: no project file's view takes it.
pub const ID: &str = "~global";

/// A row that goes to another page: its words and `›`.
pub fn link(k: &Kit, key: &str, words: &str, go: impl Fn(&mut App) + 'static) -> El {
    let el = row().key(key.to_string()).w_full().gap(12.0).center_y().py(6.0).child(text(words.to_string()).font(k.ty.body).color(k.c.text_base).grow(1.0).shrink()).child(text(crate::home::GO).font(k.ty.body).color(k.c.text_muted));
    k.interactive(el).on_press(go)
}

/// What a section says before its first answer when SERVED (its coordd takes its ask) and the project is connected;
/// none otherwise, as a project not connected says so once, on its card.
fn waiting(served: bool, k: &Kit, key: &str, title: &str, heading: &dyn Fn(String) -> El) -> Option<El> {
    served.then(|| col().key(key.to_string()).w_full().gap(2.0).child(heading(title.into())).child(text("Loading…").font(k.ty.body).color(k.c.text_muted).truncate()))
}

/// Every project as a card, with its agents running and stopped; a press opens its Home.
fn projects(s: &App, k: &Kit, heading: &dyn Fn(String) -> El) -> El {
    let cards = s.projects.iter().enumerate().map(|(i, q)| crate::home_cards::project(s, q, i, k)).collect();
    col().key("global-projects").w_full().gap(8.0).child(heading(crate::refs::count("Projects", s.projects.len()))).child(crate::home_cards::grid("global-projects-grid", cards))
}

/// The page, its numbers read through project P's coordd.
/// With no project P yet (N-1934 p12) what needs one's engine is left out: the store, and Settings.
pub fn page(s: &App, p: Option<&Proj>, k: &Kit) -> El {
    let heading = |t: String| text(t).font(k.ty.small_medium).color(k.c.text_muted);
    let pi = p.map_or(s.cur, |p| s.place_of(p));
    let up = |served: bool| p.is_some() && served && !crate::home_cards::down(s, pi);
    let store = p.and_then(|p| crate::store::section(s, p, k, &heading, true)).or_else(|| waiting(up(crate::store::served(s, pi)), k, "home-store", "Total storage", &heading));
    let cold = crate::cold::section(s, k, &heading).or_else(|| waiting(up(crate::cold::served(s, pi)), k, "home-cold", "Cold storage", &heading));
    let devices = crate::hosts::section(s, k, &heading).or_else(|| waiting(up(crate::devices::served(s, pi)), k, "home-devices", "Devices", &heading));
    let settings = p.map(|_| col().key("global-settings").w_full().gap(2.0).child(heading("Settings".into())).child(link(k, "global-settings-open", "Settings for every project", |s: &mut App| crate::policy::open(s, crate::policy::Scope::Global))));
    // under the cards: the store on the left, cold storage, devices and settings stacked on the right; the devices are
    // cards of their own, so no panel around them
    let panel = |e: El| crate::home_cards::panel(k, e);
    let side = col().grow(1.0).min_w(300.0).gap(16.0).children(cold.map(panel)).children(devices).children(settings.map(panel));
    let lower = row().key("global-lower").w_full().flex_wrap().gap(16.0).items_start().children(store.map(|e| col().grow(1.5).min_w(380.0).child(panel(e)))).child(side);
    col()
        .key("global")
        .w_full()
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(col().w_full().max_w(1080.0).gap(28.0).px(24.0).py(20.0).child(projects(s, k, &heading)).child(lower))
}

/// The Usage page's view id, beside [`ID`]: the fallback when no extension gives a Usage page ([`usage_page`]).
pub const USAGE: &str = "~usage";

/// The page the window's Usage opens: an extension's of the name (the proxy's), else this crate's.
pub fn usage_page() -> crate::Sel {
    crate::ext::window_page("usage").unwrap_or_else(|| crate::Sel::View(USAGE.into()))
}

/// What every project's agents have used, stopped ones too: the whole, then each project's.
pub fn usage(s: &App, k: &Kit) -> El {
    let heading = |t: String| text(t).font(k.ty.small_medium).color(k.c.text_muted);
    let of = |q: &Proj| q.live.agents.iter().flat_map(|a| a.usage.iter().cloned()).collect::<Vec<coord_types::ModelUsage>>();
    let all: Vec<coord_types::ModelUsage> = s.projects.iter().flat_map(of).collect();
    let mut c = col().key("usage").w_full().gap(2.0).child(heading("All projects".into()));
    c = c.child(if all.is_empty() { text("Nothing used yet").font(k.ty.body).color(k.c.text_muted) } else { row().py(4.0).child(crate::spend::spend(k, &crate::agents::spend_of(s, &all))) });
    let mut each = col().key("usage-projects").w_full().gap(2.0).child(heading("By project".into()));
    for (i, q) in s.projects.iter().enumerate() {
        let used = of(q);
        let said = if used.is_empty() { text("Nothing used yet").font(k.ty.body).color(k.c.text_muted) } else { crate::spend::spend(k, &crate::agents::spend_of(s, &used)) };
        each = each.child(row().key(format!("usage-project-{i}")).w_full().gap(12.0).center_y().py(6.0).child(text(q.name.clone()).font(k.ty.body).color(k.c.text_base).grow(1.0).shrink()).child(said));
    }
    col().key("global-usage-page").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().max_w(880.0).gap(28.0).px(24.0).py(20.0).child(c).child(each))
}

#[cfg(test)]
mod tests {
    use crate::home::tests::{keyed, made, settle, texts};
    use crate::Sel;

    /// QA N-1868.p2 #7, #12, #13: Home shows every section from its first frame, the badge says 0.4, and a project whose
    /// agents are all stopped says so.
    #[test]
    fn home_shows_every_section_before_any_answer_and_the_copy_says_what_is() {
        let (_held, mut s) = made();
        s.sel = Sel::View(super::ID.into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        for want in ["Projects · 1", "walk", "2 running · 1 stopped", "Settings for every project"] {
            assert!(t.iter().any(|x| x == want), "{want}: {t:?}");
        }
        // not connected: said once, on its card, and no panel with nothing in it
        let keys = |rt: &rune::runtime::Runtime<crate::App>, key: &str| rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref() == Some(key)).count();
        assert!(t.iter().filter(|x| x.to_lowercase().contains("not connected")).count() == 1 && keys(&rt, "global-down-0") == 1, "{t:?}");
        assert_eq!(["home-store", "home-cold", "home-devices"].map(|k| keys(&rt, k)), [0, 0, 0]);
        // connected: every section from the first frame, on its way
        let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
        rt.update(|s, _| s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap())));
        settle(&mut rt);
        let t = texts(&rt);
        for want in ["Total storage", "Cold storage", "Devices"] {
            assert!(t.iter().any(|x| x == want), "{want}: {t:?}");
        }
        assert!(!t.iter().any(|x| x.to_lowercase().contains("not connected")) && keys(&rt, "global-down-0") == 0, "{t:?}");
        // a pipe that went down, as the window's link reports it (`Incoming::Down`): the link kept, `down` said
        rt.update(|s, _| s.projects[0].down = Some("the pipe ended (3): coord: coordd is not running".into()));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(rt.state.projects[0].link.is_some() && keys(&rt, "global-down-0") == 1, "{t:?}");
        assert_eq!(t.iter().filter(|x| x.to_lowercase().contains("not connected")).count(), 1, "{t:?}");
        assert_eq!(["home-store", "home-cold", "home-devices"].map(|k| keys(&rt, k)), [0, 0, 0], "no panel waiting on a pipe that is down");
        rt.update(|s, _| (s.projects[0].link, s.projects[0].down) = (None, None));
        let _ = child.kill();
        assert!(t.iter().any(|x| x.starts_with("0.4")) && !t.iter().any(|x| x.starts_with("0.3")), "the badge: {t:?}");
        keyed(&mut rt, "global-project-0");
        rt.update(|s, _| assert_eq!(s.sel, crate::home::sel(&s.projects[0]), "its Home"));
        // every agent stopped: none running, not none at all
        rt.update(|s, _| {
            for a in &mut s.projects[0].live.agents {
                a.state = "stopped".into();
            }
        });
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "No running agents") && t.iter().any(|x| x == "3 stopped"), "{t:?}");
        assert!(!t.iter().any(|x| x.starts_with("No agent yet") || x == "and 3 stopped"), "{t:?}");
    }

    /// consult-lead-c N-1889.p3: Loading… only for an ask that goes; a linked coordd below COLD_PROTOCOL has no cold
    /// storage to wait on, so that section is not drawn, while the store's and the machines' wait.
    #[test]
    fn a_section_waits_only_on_an_ask_its_coordd_takes() {
        let (_held, mut s) = made();
        let mut child = std::process::Command::new("sh").arg("-c").arg("cat > /dev/null").stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        s.projects[0].protocol = coord_types::COLD_PROTOCOL - 1;
        s.sel = Sel::View(super::ID.into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(!t.iter().any(|x| x == "Cold storage"), "no cold storage below its protocol: {t:?}");
        assert!(t.iter().any(|x| x == "Total storage") && t.iter().any(|x| x == "Devices") && t.iter().filter(|x| *x == "Loading…").count() == 2, "{t:?}");
        rt.update(|s, _| s.projects[0].protocol = coord_types::COLD_PROTOCOL);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Cold storage") && t.iter().filter(|x| *x == "Loading…").count() == 3, "{t:?}");
        rt.update(|s, _| s.projects[0].link = None);
        drop(child.stdin.take());
        let _ = child.wait();
    }
}
