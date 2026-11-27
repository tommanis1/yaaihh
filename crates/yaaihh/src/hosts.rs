//! Home's Devices by physical host (N-1860 p2): one card per machine he owns, the red dot only when a disk of it
//! degrades; its `⋯` opens Hardware & Disk Health and Running here (the VMs, engines and services on it).

use crate::App;
use crate::devices::{Device, Held, READING, THIS_PC, about, card_figures, hardware, reach, runs_here, what_it_is};
use rune::{El, col, row, text};
use rune_kit::Kit;

/// The physical host D is or runs on: the one he names, else D itself.
pub fn of(d: &Device) -> String {
    d.host.clone().filter(|h| !h.trim().is_empty()).unwrap_or_else(|| name(d))
}

fn name(d: &Device) -> String {
    d.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "a device".into())
}

/// DEVICES by host, in the order each host first comes; a name in two cases is one host, said as it is written
/// (Windows' COMPUTERNAME is upper case, WSL's hostname as he wrote it).
pub fn grouped(devices: Vec<Device>) -> Vec<(String, Vec<Device>)> {
    let mut v: Vec<(String, Vec<Device>)> = Vec::new();
    for d in devices {
        let h = of(&d);
        match v.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(&h)) {
            Some((n, ds)) => {
                if *n == n.to_uppercase() {
                    *n = h;
                }
                ds.push(d);
            }
            None => v.push((h, vec![d])),
        }
    }
    v
}

/// What runs on D, in a word or two for the host's line: "yaaihh window", "WSL engine", "gateway", "build VM X".
fn running(d: &Device) -> Vec<String> {
    let mut v = Vec::new();
    if d.roles.iter().any(|r| r == "client") {
        v.push("yaaihh window".to_string());
    }
    if d.here == Some(true) {
        v.push(format!("{} engine", name(d)));
    }
    if d.roles.iter().any(|r| r == "jobs") {
        let vm = d.virt.as_deref().is_some_and(|v| v != "none");
        v.push(format!("{} {}", if vm { "build VM" } else { "build machine" }, name(d)));
    }
    let engines = |w: &str| w.starts_with("engine") || w == "runner";
    v.extend(d.services.iter().filter(|x| x.on != Some(false)).filter_map(|x| x.what.clone()).filter(|w| !w.is_empty() && !engines(w)));
    if !d.workers.0.is_empty() {
        v.push(format!("{} compute {}", d.workers.0.len(), if d.workers.0.len() == 1 { "worker" } else { "workers" }));
    }
    v
}

/// The Devices section for Home: one card per host; "reading…" until the first reading.
pub fn section(s: &App, k: &Kit, heading: &dyn Fn(String) -> El) -> Option<El> {
    use crate::devices::Shown;
    let d = &s.devices;
    let c = col().key("home-devices").w_full().gap(8.0);
    let (a, age) = match &d.shown {
        Shown::Asking if d.since.is_some() => return Some(c.child(heading("Devices".into())).child(text(READING.to_string()).font(k.ty.body).color(k.c.text_muted).key("devices-reading"))),
        Shown::Asking => return None,
        Shown::None(why) => return Some(c.child(heading("Devices".into())).child(text(why.clone()).font(k.ty.body).color(k.c.text_muted).truncate())),
        Shown::Answer(a, age) => (a, *age),
    };
    let hosts = grouped(crate::devices::listed(d));
    let title = crate::refs::count("Devices", hosts.len());
    let again = crate::devices::reading(d, age);
    let c = c.child(heading(match crate::devices::as_of(a, age).filter(|_| !again) {
        Some(t) => format!("{title} · {t}"),
        None => title,
    }));
    let h = crate::devices::held(&s.projects);
    let cards = hosts.iter().map(|(n, ds)| card(n, ds, d.open.contains(n), again, &h, k));
    Some(c.child(row().w_full().gap(12.0).flex_wrap().items_start().children(cards)))
}

/// Label and words rows, the label column narrow and faint; RED words in the danger colour.
fn table(rows: Vec<(String, String, bool)>, k: &Kit) -> El {
    col().w_full().gap(4.0).children(rows.into_iter().map(|(l, w, red)| {
        let words = text(w).font(k.ty.small).color(if red { k.c.danger.fg } else { k.c.text_muted }).wrap().grow(1.0).shrink().w(0.0);
        row().w_full().gap(12.0).items_start().child(text(l).font(k.ty.small).color(k.c.text_faint).w(84.0)).child(words)
    }))
}

/// A sub-section's heading inside an opened host.
fn sub(t: &str, k: &Kit) -> El {
    text(t.to_string()).font(k.ty.small_medium).color(k.c.text_base).pt(8.0)
}

/// Host NAME's card of DEVICES: its name, the red dot when a disk degrades, its `⋯`, and one line of what runs there;
/// OPEN, its hardware and disks' health, then each device on it. OLD: the reading is being read again.
fn card(host: &str, devices: &[Device], open: bool, old: bool, h: &Held, k: &Kit) -> El {
    use rune_kit::{Button, ButtonVariant};
    let degraded = devices.iter().flat_map(|d| &d.health).any(|x| x.degraded());
    let mut head = row().w_full().gap(6.0).center_y();
    if degraded {
        head = head.child(rune::el().size(6.0, 6.0).radius(3.0).fill(k.c.danger.fg).key(format!("host-mark-{host}")));
    }
    head = head.child(text(host.to_string()).font(k.ty.ui_medium).color(k.c.text_base).truncate().grow(1.0).shrink().w(0.0));
    let key = host.to_string();
    let more = Button::new("⋯").variant(ButtonVariant::Ghost).expanded(open).help("Hardware and what runs here").on_press(move |s: &mut App| {
        if !s.devices.open.remove(&key) {
            s.devices.open.insert(key.clone());
        }
    });
    head = head.child(more.build(k).key(format!("host-more-{host}")));
    let line = devices.iter().flat_map(running).collect::<Vec<_>>().join(" · ");
    let mut card = col().w(400.0).gap(8.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(head);
    if !line.is_empty() {
        card = card.child(text(line).font(k.ty.small).color(k.c.text_faint).wrap());
    }
    // a device out of reach says so on the host's face, in words, not with the dot
    for d in devices.iter().filter(|_| !old) {
        if let Some((words, true)) = reach(d) {
            card = card.child(text(format!("{}: {words}", name(d))).font(k.ty.small).color(k.c.danger.fg).wrap());
        }
    }
    if open {
        card = card.child(opened(devices, old, h, k).key(format!("host-open-{host}")));
    }
    col().key(format!("host-{host}")).child(card)
}

/// An opened host: Hardware & Disk Health (its own machine's, never a VM's share), then Running here.
fn opened(devices: &[Device], old: bool, h: &Held, k: &Kit) -> El {
    // the machine itself: not a VM, not a build machine (those may be VMs not saying so)
    let bare = devices.iter().find(|d| matches!(d.virt.as_deref(), None | Some("none")) && !d.roles.iter().any(|r| r == "jobs"));
    let mut hw: Vec<(String, String, bool)> = bare.map(|d| hardware(d).into_iter().map(|(l, w)| (l, w, false)).collect()).unwrap_or_default();
    for x in devices.iter().flat_map(|d| &d.health) {
        let words = [x.name.clone(), Some(x.words())].into_iter().flatten().filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" · ");
        hw.push(("Disk health".into(), words, x.degraded()));
    }
    if bare.is_none() {
        hw.push(("Not read".into(), "its own system: that needs a login there".into(), false));
    } else if devices.iter().all(|d| d.health.is_empty()) {
        hw.push(("Disk health".into(), "not read on this machine".into(), false));
    }
    let mut c = col().w_full().gap(6.0).pt(8.0).border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_weak).child(sub("Hardware & Disk Health", k)).child(table(hw, k)).child(sub("Running here", k));
    for d in devices {
        c = c.child(running_here(d, old, h, k));
    }
    c
}

/// Device D under its host: its name and what it is, what runs there, its figures as bars, what else is said of it.
fn running_here(d: &Device, old: bool, h: &Held, k: &Kit) -> El {
    let n = name(d);
    let shown = if n == THIS_PC { "Windows".to_string() } else { n.clone() };
    let mut head = row().w_full().gap(6.0).center_y().child(text(shown).font(k.ty.small_medium).color(k.c.text_base).grow(1.0).shrink().w(0.0));
    let what = what_it_is(d);
    if !what.is_empty() {
        head = head.child(text(what).font(k.ty.small).color(k.c.text_muted));
    }
    let mut c = col().key(format!("device-{n}")).w_full().gap(6.0).pt(4.0).child(head);
    let here = runs_here(d, h);
    if !here.is_empty() {
        c = c.child(text(here).font(k.ty.small).color(k.c.text_faint).wrap());
    }
    c = c.children(crate::compute::lines(&d.workers, &n, k));
    if old {
        return c.child(text(READING.to_string()).font(k.ty.small).color(k.c.text_muted));
    }
    for f in card_figures(d) {
        c = c.child(crate::devices::figure_line(&f, k));
    }
    let more: Vec<(String, String, bool)> = about(d).into_iter().map(|(l, w)| (l, w, false)).collect();
    if !more.is_empty() {
        c = c.child(table(more, k));
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device with no host is a host of its own, under its own name; a host in two cases is one, said as written.
    #[test]
    fn a_host_is_only_as_he_names_it() {
        let d = |n: &str, h: Option<&str>, virt: &str| Device { name: Some(n.into()), host: h.map(Into::into), virt: Some(virt.into()), ..Device::default() };
        let g = grouped(vec![d("WSL", None, "wsl"), d("build-1", None, "kvm"), d("PC", Some("DESK"), "none"), d("build-2", Some("Desk"), "kvm")]);
        let names: Vec<(&str, usize)> = g.iter().map(|(h, ds)| (h.as_str(), ds.len())).collect();
        assert_eq!(names, [("WSL", 1), ("build-1", 1), ("Desk", 2)]);
    }
}
