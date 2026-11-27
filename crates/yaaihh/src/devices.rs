//! Home's Devices (the owner, N-1104 p1070: "i want to add the devices themselves eh . separate server from clients";
//! the devices page he was shown, N-1104 p1093, and his "yes" to its questions 1 and 2, p1096, p1097; paper-research
//! N-1446 p2772, p2804): each machine of his, what it does (server of his projects, a machine for jobs, his window),
//! how loaded and full it is, and what runs there. A red mark only where a device needs him: it does not
//! answer, a disk of it is nearly full, it is too hot. A press on a device opens what else it has, and closes it.
//! One card per machine (his N-1104 p1166, "i prefer nice ui and ux os eg oen card per machien stats temps etc";
//! paper-research N-1446 p3058): what it is, what runs there, CPU, memory, each disk and its temperatures as small
//! bars, whether it is reached; "reading…" until the first reading.
//!
//! The section measures nothing of the server's machines: coordd hands over what its reader read (`Request::Machines`,
//! docket-edge7's step 1, H-1594), the fields agreed with docket-edge7 (N-1446 p2814), each optional, one that is
//! missing or of another type left out of its line, never guessed. The reading and the rows are here-2's, from its
//! Machines page (9dcb1970), whose page went (p2777). What the window holds already is not in the answer and is taken
//! from the window (p2814, 4): his projects and their agents, which the engine's own device serves, and the agents' jobs
//! on the runner by the machine each went to (`AgentView.jobs`). The window's own machine, which a server on another
//! machine cannot see, the window reads itself ([`this_pc`]): its memory and its disk C:, on Windows (p2814, 2).
//!
//! Records stay out until his answer to the page's question 3 (p2804).

use crate::{App, Sel};
use rune::{El, Tx, col, row, text};
use rune_kit::Kit;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// How often Home asks while it shows (here-2's Machines page's).
const EVERY: Duration = Duration::from_secs(3);

/// How often the window's own machine is read while Home shows, apart from the UI loop: its disk's health log can
/// take seconds (N-1933.p1: no probe on the UI loop).
const THIS_PC_EVERY: Duration = Duration::from_secs(30);

/// A disk with less of it free than this share needs him: the share at which File Explorer draws a drive's bar red, so
/// he sees the same red in both places (docket-edge7 p2814, 3).
const DISK_LOW: f64 = 0.10;

/// A reading this old or older while Home asks every [`EVERY`] means the reader has stopped (docket-edge7 p2814, 3):
/// the section says "as of HH:MM" over it rather than drawing old numbers as current.
const STALE_SECS: u64 = 60;

/// How long, from the section's first ask, an old reading is taken as being read again ([`reading`]): the sampler's
/// worst run, about 50 s (docket-core, H-1594 p3), and the ask after it.
const READING_WAIT: Duration = Duration::from_secs(60);

/// What the section and a card say while the first reading comes (p3058).
pub(crate) const READING: &str = "reading…";

/// The window's own machine: Windows' own name for it, since the window cannot know what it is (p2814, 2).
pub(crate) const THIS_PC: &str = "This PC · Windows";

/// A field taken when it reads as T, else none: a field of another type leaves only itself out (here-2's).
pub(crate) fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    Ok(serde_json::from_value(Value::deserialize(d)?).ok())
}

/// A list taken item by item: an item that does not read is left out (here-2's).
pub(crate) fn items<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(v) => v.into_iter().filter_map(|x| serde_json::from_value(x).ok()).collect(),
        _ => Vec::new(),
    })
}

/// What coordd's reader read (docket-edge7 p2814): when, and the devices. Its `build_machine` is the engine's jobs
/// reader's, not the window's.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Answer {
    #[serde(default, deserialize_with = "lenient")]
    pub read: Option<String>,
    #[serde(default, deserialize_with = "items")]
    pub devices: Vec<Device>,
}

/// A device: a machine with a name and what it does there.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Device {
    /// "WSL" or "server" for the engine's own device, the runner's own name for a machine for jobs ("yaaihh-build-1",
    /// as a job's `machine` says it); his names from projects.toml replace them (devices page, step 2).
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    /// The device the engine that answered runs on.
    #[serde(default, deserialize_with = "lenient")]
    pub here: Option<bool>,
    /// What it runs under, as systemd-detect-virt says it: `wsl`, `kvm`, … (`none` on a machine of its own).
    #[serde(default, deserialize_with = "lenient")]
    pub virt: Option<String>,
    /// `server` (of his projects), `jobs` (a machine for jobs), `client` (his window).
    #[serde(default, deserialize_with = "items")]
    pub roles: Vec<String>,
    /// True when it answered; false when it did not; none when not known.
    #[serde(default, deserialize_with = "lenient")]
    pub on: Option<bool>,
    /// When it was asked.
    #[serde(default, deserialize_with = "lenient")]
    pub asked: Option<String>,
    /// The last time it answered, when it does not now.
    #[serde(default, deserialize_with = "lenient", alias = "since")]
    pub last_on: Option<String>,
    /// Why it says nothing, when `on` is false ("no answer").
    #[serde(default, deserialize_with = "lenient")]
    pub why: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub load1: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub cores: Option<f64>,
    /// Its CPU's model, as /proc/cpuinfo or Windows names it.
    #[serde(default, deserialize_with = "lenient")]
    pub cpu_model: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub mem_used_gb: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub mem_total_gb: Option<f64>,
    /// Its hottest sensor, and the heat its sensor says is critical (hwmon's own `crit`, else `max`): none on a machine
    /// with no sensor (WSL, a VM), never 0.
    #[serde(default, deserialize_with = "lenient")]
    pub temp_c: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub temp_crit_c: Option<f64>,
    /// Every sensor it gives, the hottest first, each with its own critical heat (the sampler's `temps`: hwmon's and
    /// the thermal zones'; p3058); none from a reader before it, which gave the hottest alone.
    #[serde(default, deserialize_with = "items")]
    pub temps: Vec<Temp>,
    #[serde(default, deserialize_with = "lenient")]
    pub gpu: Option<Gpu>,
    #[serde(default, deserialize_with = "items")]
    pub disks: Vec<Disk>,
    /// Which of ours run there: the engines, the gateway, its usage reader, the runner, the relay.
    #[serde(default, deserialize_with = "items")]
    pub services: Vec<Service>,
    /// A machine for jobs: whether its runner runs.
    #[serde(default, deserialize_with = "lenient")]
    pub runner: Option<bool>,
    /// The compute workers coordd found on it (compute.rs).
    #[serde(default)]
    pub workers: crate::compute::Workers,
    /// The physical machine it is or runs on: its own name, or yaaihh.toml's `[[device]] host` ([`crate::hosts::of`]).
    #[serde(default, deserialize_with = "lenient")]
    pub host: Option<String>,
    /// Its physical disks' health (disk_health.rs), each only where it was read.
    #[serde(default, deserialize_with = "items")]
    pub health: Vec<crate::disk_health::DiskHealth>,
}

/// One sensor: its name as the machine gives it ("k10temp Tctl", "acpitz"), its heat and its critical heat, in °C.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Temp {
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub c: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub crit_c: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Gpu {
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub temp_c: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub busy_pct: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub mem_used_gb: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub mem_total_gb: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Disk {
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub free_gb: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    pub total_gb: Option<f64>,
}

/// One of ours on a device: its unit, and `what` it is in his words ("engine research", "runner", "gateway", "usage
/// reader", "relay"), which is what the window says; never the unit (p2814, 4).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Service {
    #[serde(default, deserialize_with = "lenient")]
    pub unit: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub what: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub on: Option<bool>,
}

/// coordd's answer as machine-state's machines.rs gives it: the reader's text and its age, else why there is none.
#[derive(Deserialize)]
struct Served {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    age_secs: Option<u64>,
    #[serde(default)]
    why: Option<String>,
}

/// What the section has to show.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Shown {
    /// Nothing asked yet, or no coordd of his serves it: no section.
    #[default]
    Asking,
    /// The devices, and how old the reading was.
    Answer(Answer, Option<u64>),
    /// None to show: coordd's why, or what could not be read (one plain line).
    None(String),
}

/// The section's state (`App::devices`).
pub struct Devices {
    pub shown: Shown,
    /// When coordd was last asked.
    asked: Option<Instant>,
    /// When coordd was first asked for it: "reading…" until a reading comes, and while an old one is read again
    /// ([`reading`]).
    pub since: Option<Instant>,
    /// The hosts whose `⋯` is open, by name.
    pub open: BTreeSet<String>,
    /// The window's own machine as it read it at the last ask ([`this_pc`]).
    pub this_pc: Option<Device>,
    /// What reads it: [`this_pc`]; a test's stand-in.
    pub reader: fn() -> Option<Device>,
    /// The reading on its way from the reader, and when the last one ended.
    reading: Option<std::sync::mpsc::Receiver<Option<Device>>>,
    read_at: Option<Instant>,
}

impl Default for Devices {
    fn default() -> Self {
        Devices { shown: Shown::Asking, asked: None, since: None, open: BTreeSet::new(), this_pc: None, reader: this_pc, reading: None, read_at: None }
    }
}

/// What the window itself holds that the answer leaves to it (docket-edge7 p2814, 4): its projects, each with the agents
/// that run or wait (Home's own count, home.rs), which the engine's own device serves; and its agents' jobs on the runner
/// that have not ended, running and waiting, by the machine each went to (none: the runner on the engine's own machine).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held {
    pub projects: Vec<(String, usize)>,
    pub jobs: BTreeMap<Option<String>, (usize, usize)>,
}

/// What the window holds of PROJECTS for the section ([`Held`]). A job whose machine does not answer is in no count:
/// its machine's row says it does not answer.
pub fn held(projects: &[crate::Proj]) -> Held {
    let mut h = Held::default();
    for p in projects {
        h.projects.push((p.name.clone(), p.live.agents.iter().filter(|a| a.state != "stopped").count()));
        for j in p.live.agents.iter().flat_map(|a| &a.jobs).filter(|j| j.runs()) {
            let n = h.jobs.entry(j.machine.clone()).or_default();
            match j.step.as_str() {
                "queued" => n.1 += 1,
                "unreachable" => {}
                _ => n.0 += 1,
            }
        }
    }
    h
}

/// Whether project PI's machines are read: by the v2 project's reader, or its coordd takes `Request::Machines` and its
/// pipe is up.
pub(crate) fn served(s: &App, pi: usize) -> bool {
    crate::machine::reads(s, pi) || s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::MACHINES_PROTOCOL && p.link.is_some())
}

/// Whether Home shows: the current project's page is its Home.
fn home_shows(s: &App) -> bool {
    let own = matches!(&s.sel, Sel::View(v) if v == crate::global::ID || v == crate::policy::ID);
    own || s.projects.get(s.cur).is_some_and(|p| s.sel == crate::home::sel(p))
}

/// While Home shows: ask the current project's coordd every [`EVERY`], and read this machine every [`THIS_PC_EVERY`]
/// apart from the UI loop; and its layers, at their own pace (layers.rs `ask`). Nothing at all while it does not.
pub fn poll(s: &mut App, tx: &mut Tx) -> bool {
    let read = crate::machine::came(s) | this_pc_came(s);
    if !home_shows(s) {
        return read;
    }
    if s.devices.reading.is_none() && s.devices.read_at.is_none_or(|t| t.elapsed() >= THIS_PC_EVERY) {
        let (put, got) = std::sync::mpsc::channel();
        let (reader, wake) = (s.devices.reader, s.wake.clone());
        std::thread::spawn(move || {
            let _ = put.send(reader());
            if let Some(w) = wake {
                w();
            }
        });
        s.devices.reading = Some(got);
    }
    s.wake_every(EVERY, tx);
    // a press on a Store row asks its page at once, not at the next round
    crate::store::ask_page(s, s.cur);
    // and the settings once, and the repositories' state at their own pace, at once, not at the next round
    crate::policy::ask(s, s.cur);
    crate::home::ask_repo_state(s, s.cur);
    if s.devices.asked.is_some_and(|t| t.elapsed() < EVERY) {
        return read;
    }
    s.devices.asked = Some(Instant::now());
    let pi = s.cur;
    // with its pipe down nothing is asked and nothing said: the section keeps what it showed
    if crate::machine::reads(s, pi) {
        crate::machine::ask(s, crate::machine::Ask::Machines, 0);
        s.devices.since.get_or_insert_with(Instant::now);
    } else if served(s, pi) {
        s.request_in(pi, coord_types::Request::Machines, crate::Pending::Devices);
        s.devices.since.get_or_insert_with(Instant::now);
    }
    crate::layers::ask(s, pi);
    crate::store::ask(s, pi);
    crate::cold::ask(s, pi);
    read
}

/// The reader's reading, when it came: whether it changed what the section shows.
fn this_pc_came(s: &mut App) -> bool {
    let Some(got) = &s.devices.reading else { return false };
    match got.try_recv() {
        Ok(d) => {
            (s.devices.reading, s.devices.read_at) = (None, Some(Instant::now()));
            let was = std::mem::replace(&mut s.devices.this_pc, d);
            was != s.devices.this_pc
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => false,
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            (s.devices.reading, s.devices.read_at) = (None, Some(Instant::now()));
            false
        }
    }
}

/// coordd's answer R.
pub fn came(s: &mut App, r: Result<String, String>) {
    s.devices.shown = read(r);
}

/// What coordd's answer R shows.
fn read(r: Result<String, String>) -> Shown {
    let served = match r.and_then(|out| serde_json::from_str::<Served>(&out).map_err(|e| format!("the server's answer: {e}"))) {
        Ok(v) => v,
        Err(e) => return Shown::None(crate::accounts::plain(&e)),
    };
    match served.text {
        None => Shown::None(served.why.unwrap_or_else(|| "no reading yet".into())),
        Some(t) => match serde_json::from_str::<Answer>(&t) {
            Ok(a) => Shown::Answer(a, served.age_secs),
            Err(e) => Shown::None(format!("the reading could not be read: {e}")),
        },
    }
}

/// The devices Home lists: the window's own machine first, as the page has it (p1093), then coordd's; none while coordd
/// has given no reading, not even the window's own.
pub fn listed(d: &Devices) -> Vec<Device> {
    let Shown::Answer(a, _) = &d.shown else { return Vec::new() };
    d.this_pc.iter().chain(&a.devices).cloned().collect()
}

/// "as of 18:01" when the reading is [`STALE_SECS`] old or older: when it was read, else its age from now.
pub(crate) fn as_of(a: &Answer, age: Option<u64>) -> Option<String> {
    let age = age.filter(|s| *s >= STALE_SECS)?;
    let at = a.read.clone().unwrap_or_else(|| (chrono::Utc::now() - chrono::Duration::seconds(age as i64)).to_rfc3339());
    Some(format!("as of {}", crate::when::at(&at)))
}

/// Whether D holds a figure of its own.
fn measured(d: &Device) -> bool {
    d.load1.is_some() || d.mem_used_gb.is_some() || d.mem_total_gb.is_some() || !d.disks.is_empty()
}

/// A number as he reads it: whole when it is near one or a hundred and more, else one decimal, as the page he was shown
/// says them ("18.8 of 39 GB", "25 GB free of 953"; here-2's, whole from ten).
fn num(x: f64) -> String {
    if (x - x.round()).abs() < 0.05 || x >= 100.0 { format!("{}", x.round() as i64) } else { format!("{x:.1}") }
}

/// Whether D answered: said `on`, or gave figures without saying (here-2's; his window's own machine always).
fn answers(d: &Device) -> bool {
    d.on == Some(true) || (d.on.is_none() && measured(d))
}

/// What D's line says when it did not answer: its why, "no answer" when it gives none, and since when; never "off",
/// which is not known (here-2's, consult-lead N-1136 post 10).
fn no_answer(d: &Device) -> String {
    let why = d.why.clone().filter(|w| !w.is_empty()).unwrap_or_else(|| "no answer".into());
    match &d.last_on {
        Some(t) => format!("{why} since {}", crate::when::at(t)),
        None => why,
    }
}

/// D's disks that need him: less than [`DISK_LOW`] of them free.
fn low_disks(d: &Device) -> Vec<&Disk> {
    d.disks.iter().filter(|k| matches!((k.free_gb, k.total_gb), (Some(f), Some(t)) if t > 0.0 && f / t < DISK_LOW)).collect()
}

/// Whether D is too hot: at or over the heat its sensor says is critical.
fn too_hot(d: &Device) -> bool {
    matches!((d.temp_c, d.temp_crit_c), (Some(t), Some(c)) if c > 0.0 && t >= c)
}

/// Whether D needs him: it was asked and did not answer, a disk of it is nearly full, or it is too hot.
pub fn needs_you(d: &Device) -> bool {
    d.on == Some(false) || !low_disks(d).is_empty() || too_hot(d)
}

/// What runs on D, the page's "runs here", from what the window holds (H): "server of research (14 agents), cav-paper"
/// on the engine's own device, "2 jobs running, 1 waiting" (or "no jobs") on a machine for jobs, that its runner does
/// not run, "your window" on his window's.
pub fn runs_here(d: &Device, h: &Held) -> String {
    let mut parts = Vec::new();
    let engine = d.here == Some(true);
    if engine && !h.projects.is_empty() {
        let served: Vec<String> = h
            .projects
            .iter()
            .map(|(name, a)| match a {
                0 => name.clone(),
                1 => format!("{name} (1 agent)"),
                a => format!("{name} ({a} agents)"),
            })
            .collect();
        parts.push(format!("server of {}", served.join(", ")));
    }
    // a nameless device is no job's machine; the engine's own takes the jobs that name none
    let named = d.name.as_ref().and_then(|_| h.jobs.get(&d.name)).copied().unwrap_or_default();
    let mine = if engine { h.jobs.get(&None).copied().unwrap_or_default() } else { (0, 0) };
    let (r, w) = (named.0 + mine.0, named.1 + mine.1);
    if r + w > 0 || d.roles.iter().any(|x| x == "jobs") {
        // beside compute workers the runner's jobs are named as its own
        let (one, many) = if d.workers.0.is_empty() { ("job", "jobs") } else { ("runner job", "runner jobs") };
        let mut said = Vec::new();
        if r > 0 {
            said.push(format!("{r} {} running", if r == 1 { one } else { many }));
        }
        if w > 0 {
            said.push(if said.is_empty() { format!("{w} {} waiting", if w == 1 { one } else { many }) } else { format!("{w} waiting") });
        }
        parts.push(if said.is_empty() { format!("no {many}") } else { said.join(", ") });
    }
    if d.runner == Some(false) {
        parts.push("its runner does not run".into());
    }
    if parts.is_empty() && d.roles.iter().any(|x| x == "client") {
        parts.push("your window".into());
    }
    parts.join(" · ")
}

/// One figure of a device's card: NAME at the left, WORDS at the right, under them a bar filled to FRAC when it has a
/// most to measure against, and whether it needs him (a disk nearly full, too hot).
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    pub name: String,
    pub words: String,
    pub frac: Option<f32>,
    pub needs: bool,
}

/// D's card figures (paper-research N-1446 p3058; his N-1104 p1166, "one card per machien stats temps"): CPU load
/// against its cores, memory against all of it, each disk by how much of it is used, and its hottest sensor
/// against the heat its sensor calls critical; none when it does not answer, which its card says instead.
pub fn card_figures(d: &Device) -> Vec<Figure> {
    if !answers(d) {
        return Vec::new();
    }
    let mut v = Vec::new();
    let share = |a: f64, b: Option<f64>| b.filter(|b| *b > 0.0).map(|b| (a / b).clamp(0.0, 1.0) as f32);
    if let Some(l) = d.load1 {
        let words = match d.cores {
            Some(c) => format!("load {} of {} cores", num(l), num(c)),
            None => format!("load {}", num(l)),
        };
        v.push(Figure { name: "CPU".into(), words, frac: share(l, d.cores), needs: false });
    }
    match (d.mem_used_gb, d.mem_total_gb) {
        (Some(u), Some(t)) => v.push(Figure { name: "Memory".into(), words: format!("{} of {} GB", num(u), num(t)), frac: share(u, Some(t)), needs: false }),
        (Some(u), None) => v.push(Figure { name: "Memory".into(), words: format!("{} GB used", num(u)), frac: None, needs: false }),
        (None, Some(t)) => v.push(Figure { name: "Memory".into(), words: format!("{} GB", num(t)), frac: None, needs: false }),
        (None, None) => {}
    }
    let low = low_disks(d);
    for k in &d.disks {
        let name = match k.name.as_deref().filter(|n| !n.is_empty()) {
            Some(n) => format!("Disk {n}"),
            None => "Disk".to_string(),
        };
        let (words, frac) = match (k.free_gb, k.total_gb) {
            (Some(f), Some(t)) => (format!("{} GB free of {}", num(f), num(t)), (t > 0.0).then(|| (1.0 - f / t).clamp(0.0, 1.0) as f32)),
            (Some(f), None) => (format!("{} GB free", num(f)), None),
            (None, Some(t)) => (format!("{} GB", num(t)), None),
            (None, None) => continue,
        };
        v.push(Figure { name, words, frac, needs: low.contains(&k) });
    }
    // the hottest of the sensors it gives (the sampler's `temps`, hottest first), against its own critical heat; the
    // others behind its `…` ([`specs`]). A sample with no `temps` (a sampler from before) has no temperature row
    // (paper-research N-1446 p3125)
    if let Some((t, c)) = d.temps.iter().find_map(|t| Some((t, t.c?))) {
        let words = format!("{} °C {}", num(c), t.name.clone().unwrap_or_default()).trim_end().to_string();
        v.push(Figure { name: "Temperature".into(), words, frac: share(c, t.crit_c), needs: too_hot(d) });
    }
    v
}

/// What D is, in his words (p3058: "server, client, build machine"), each once, in that order.
pub fn what_it_is(d: &Device) -> String {
    [("server", "server"), ("jobs", "build machine"), ("client", "client")]
        .iter()
        .filter(|(role, _)| d.roles.iter().any(|r| r == role))
        .map(|(_, word)| *word)
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Whether it can be reached, in a line: "reached" when it answered, else why not and since when (the danger colour);
/// none for his window's own machine, which the window reads itself.
pub(crate) fn reach(d: &Device) -> Option<(String, bool)> {
    if d.on.is_none() && d.roles.iter().all(|r| r == "client") {
        return None;
    }
    Some(if answers(d) { ("reached".to_string(), false) } else { (no_answer(d), d.on == Some(false)) })
}

/// D's hardware, a label and its words each: CPU, graphics card, memory, each disk's room, its sensors; what is not
/// read is left out.
pub(crate) fn hardware(d: &Device) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    let mut add = |l: &str, w: String| v.push((l.to_string(), w));
    let cpu: Vec<String> = [d.cpu_model.clone().filter(|m| !m.is_empty()), d.cores.map(|c| format!("{} cores", num(c)))].into_iter().flatten().collect();
    if !cpu.is_empty() {
        add("CPU", cpu.join(" · "));
    }
    if let Some(g) = &d.gpu {
        let mut parts = vec![g.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "graphics card".into())];
        match (g.mem_used_gb, g.mem_total_gb) {
            (Some(u), Some(t)) => parts.push(format!("{} GB, {} used", num(t), num(u))),
            (None, Some(t)) => parts.push(format!("{} GB", num(t))),
            _ => {}
        }
        parts.extend(g.busy_pct.map(|b| format!("busy {}%", num(b))));
        parts.extend(g.temp_c.map(|t| format!("{} °C", num(t))));
        add("Graphics", parts.join(" · "));
    }
    if let Some(t) = d.mem_total_gb {
        add("Memory", format!("{} GB", num(t)));
    }
    for k in &d.disks {
        let name = k.name.as_deref().filter(|n| !n.is_empty()).map_or("Disk".to_string(), |n| format!("Disk {n}"));
        match (k.free_gb, k.total_gb) {
            (Some(f), Some(t)) => add(&name, format!("{} GB free of {}", num(f), num(t))),
            (None, Some(t)) => add(&name, format!("{} GB", num(t))),
            (Some(f), None) => add(&name, format!("{} GB free", num(f))),
            (None, None) => {}
        }
    }
    let sensors: Vec<String> = d.temps.iter().filter_map(|t| Some(format!("{} °C {}", num(t.c?), t.name.clone().unwrap_or_default()).trim_end().to_string())).collect();
    if !sensors.is_empty() {
        let crit = d.temps.iter().find(|t| t.c.is_some()).and_then(|t| t.crit_c).map(|c| format!(" (critical at {} °C)", num(c)));
        add("Sensors", format!("{}{}", sensors.join(" · "), crit.unwrap_or_default()));
    }
    v
}

/// What else of D is said: what it runs under, which of ours run there, when it was asked.
pub(crate) fn about(d: &Device) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = Vec::new();
    let mut add = |l: &str, w: String| v.push((l.to_string(), w));
    // "wsl" on the device named WSL is said by its name
    match d.virt.as_deref().filter(|x| !x.is_empty() && *x != "none") {
        Some("wsl") if d.name.as_deref() == Some("WSL") => {}
        Some(x) => add("Runs under", x.to_string()),
        None => {}
    }
    let ours: Vec<String> = d
        .services
        .iter()
        .filter_map(|x| {
            let what = x.what.clone().filter(|n| !n.is_empty())?;
            Some(match x.on {
                Some(true) => format!("{what} runs"),
                Some(false) => format!("{what} does not run"),
                None => what,
            })
        })
        .collect();
    if !ours.is_empty() {
        add("Ours", ours.join(" · "));
    }
    if let Some(t) = &d.asked {
        add("Asked", crate::when::at(t));
    }
    v
}

/// Whether the reading is an old one being read again: [`STALE_SECS`] old or older, while the section has asked for
/// less than [`READING_WAIT`] (the engine starts its sampler at the first ask that finds the file old, and its fresh
/// sample comes within about 50 s). Then the engine's devices say "reading…", not numbers days old (docket-edge7's walk,
/// N-1446 p3050, 9: "as of Sun 00:31" at a first look); after it, the old numbers with "as of" over them, as before.
pub(crate) fn reading(d: &Devices, age: Option<u64>) -> bool {
    age.is_some_and(|a| a >= STALE_SECS) && d.since.is_some_and(|t| t.elapsed() < READING_WAIT)
}

/// A card's figure F: its name and words on one line, its bar under them, quiet unless it needs him (window rules: a
/// full bar is no alarm by itself; the danger colour only where a device needs him, paper-research's Right on H-1605).
pub(crate) fn figure_line(f: &Figure, k: &Kit) -> El {
    let color = if f.needs { k.c.danger.fg } else { k.c.text_muted };
    let line = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .justify_between()
        .child(text(f.name.clone()).font(k.ty.small).color(k.c.text_base))
        .child(text(f.words.clone()).font(k.ty.small).color(color).truncate().shrink());
    col().w_full().gap(3.0).child(line).children(f.frac.map(|frac| bar(frac, f.needs, &f.name, k)))
}

/// The one bar of Home's figures (a device card's, a Store row's, paper-research's read on H-1702): FRAC of its track,
/// in the danger colour when it NEEDS him, else faint; read out as "NAME: N%".
pub(crate) fn bar(frac: f32, needs: bool, name: &str, k: &Kit) -> El {
    let fill = if needs { k.c.danger.icon } else { k.c.text_faint };
    row().w_full().h(4.0).radius(2.0).fill(k.c.layer_02).child(rune::el().h(4.0).radius(2.0).w_frac(frac.clamp(0.0, 1.0)).fill(fill)).label(format!("{name}: {}%", (frac * 100.0).round() as i64))
}

/// The window's own machine, read where it runs: its CPU, memory and disk C:, on Windows (GlobalMemoryStatusEx,
/// GetDiskFreeSpaceExW), as Task Manager and File Explorer count them (GB of 2^30 bytes). No load: Windows keeps no load
/// average. Elsewhere none: there the server's reader sees the machine the window runs on. `YAAIHH_THIS_PC` (a device as
/// JSON) stands in for it, for a still, which never shows his machine's own figures.
pub fn this_pc() -> Option<Device> {
    if let Ok(j) = std::env::var("YAAIHH_THIS_PC") {
        return serde_json::from_str(&j).ok();
    }
    read_this_pc()
}

#[cfg(windows)]
fn read_this_pc() -> Option<Device> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::core::w;
    const GB: f64 = (1u64 << 30) as f64;
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    let mem = unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok().then(|| (m.ullTotalPhys.saturating_sub(m.ullAvailPhys) as f64 / GB, m.ullTotalPhys as f64 / GB));
    let (mut free, mut total) = (0u64, 0u64);
    let disk = unsafe { GetDiskFreeSpaceExW(w!("C:\\"), None, Some(&mut total as *mut u64), Some(&mut free as *mut u64)) }
        .is_ok()
        .then(|| Disk { name: Some("C:".into()), free_gb: Some(free as f64 / GB), total_gb: Some(total as f64 / GB) });
    if mem.is_none() && disk.is_none() {
        return None;
    }
    Some(Device {
        name: Some(THIS_PC.into()),
        // the machine's own name, as WSL on it names its host
        host: std::env::var("COMPUTERNAME").ok(),
        roles: vec!["client".into()],
        cores: std::thread::available_parallelism().ok().map(|n| n.get() as f64),
        cpu_model: cpu_model(),
        mem_used_gb: mem.map(|m| m.0),
        mem_total_gb: mem.map(|m| m.1),
        disks: disk.into_iter().collect(),
        health: crate::disk_health::here(),
        ..Device::default()
    })
}

/// The CPU's name as Windows keeps it (System Information's "Processor").
#[cfg(windows)]
fn cpu_model() -> Option<String> {
    use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::w;
    let mut buf = [0u16; 128];
    let mut len = std::mem::size_of_val(&buf) as u32;
    let key = w!("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0");
    let r = unsafe { RegGetValueW(HKEY_LOCAL_MACHINE, key, w!("ProcessorNameString"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut len)) };
    let n = buf.iter().position(|c| *c == 0).unwrap_or(0);
    r.is_ok().then(|| String::from_utf16_lossy(&buf[..n]).trim().to_string()).filter(|m| !m.is_empty())
}

#[cfg(not(windows))]
fn read_this_pc() -> Option<Device> {
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// The reader's devices in docket-edge7's fields (N-1446 p2814), in coordd's answer (`text`, `age_secs`): the
    /// engine's own device in WSL, the build machine answering with its runner, and a second machine for jobs not
    /// answering, its heat of another type.
    pub(crate) fn answer() -> String {
        let text = json!({"read": "2026-11-24T17:01:02Z", "devices": [
            {"name": "WSL", "host": "Laptop", "here": true, "virt": "wsl", "roles": ["server"], "on": true, "asked": "2026-11-24T17:01:02Z",
             "load1": 3.6, "cores": 16, "cpu_model": "AMD Ryzen 9 7950X 16-Core Processor", "mem_used_gb": 18.8, "mem_total_gb": 39.2, "disks": [{"name": "/", "free_gb": 228.9, "total_gb": 688.0}],
             "services": [{"unit": "coordd-research", "what": "engine research", "on": true}, {"unit": "cpa-usage", "what": "usage reader", "on": false}]},
            {"name": "yaaihh-build-1", "host": "Tower", "virt": "kvm", "roles": ["jobs"], "on": true, "load1": 9, "cores": 20, "mem_used_gb": 11, "mem_total_gb": 20,
             "runner": true, "services": [{"unit": "build-runner", "what": "runner", "on": true}]},
            {"name": "yaaihh-build-2", "host": "Tower", "roles": ["jobs"], "on": false, "last_on": "2026-11-24T16:00:00Z", "why": "no answer", "runner": false, "temp_c": "warm"}
        ]})
        .to_string();
        json!({"text": text, "age_secs": 4}).to_string()
    }

    /// His window's machine as the window reads it: its name as Windows says it, its memory and C:, 25 GB free of 953.
    pub(crate) fn me() -> Device {
        Device {
            name: Some(THIS_PC.into()),
            host: Some("LAPTOP".into()),
            roles: vec!["client".into()],
            mem_used_gb: Some(20.4),
            mem_total_gb: Some(31.7),
            disks: vec![Disk { name: Some("C:".into()), free_gb: Some(25.0), total_gb: Some(953.0) }],
            ..Device::default()
        }
    }

    /// What the window holds: two projects on the engine's machine, and three jobs on the build machine, one queued.
    fn held_here() -> Held {
        Held { projects: vec![("research".into(), 14), ("cav-paper".into(), 0)], jobs: BTreeMap::from([(Some("yaaihh-build-1".into()), (2, 1))]) }
    }

    /// N-1933.p1: a slow reading of this machine (its disk's health log) never holds the UI loop;
    /// it lands at a later poll, and is not begun again within `THIS_PC_EVERY`. Away from Home nothing is read.
    #[test]
    fn this_machine_is_read_apart_from_the_ui_loop() {
        fn slow() -> Option<Device> {
            std::thread::sleep(Duration::from_millis(1500));
            Some(Device { name: Some("slow".into()), ..Device::default() })
        }
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev"]);
        s.devices.reader = slow;
        s.sel = crate::Sel::Tickets;
        poll(&mut s, &mut rune::Tx::default());
        assert!(s.devices.reading.is_none(), "away from Home: nothing read");
        s.sel = crate::home::sel(&s.projects[0]);
        let began = Instant::now();
        poll(&mut s, &mut rune::Tx::default());
        assert!(began.elapsed() < Duration::from_millis(500), "the poll waited {:?}", began.elapsed());
        assert!(s.devices.reading.is_some() && s.devices.this_pc.is_none());
        let landed = loop {
            std::thread::sleep(Duration::from_millis(100));
            poll(&mut s, &mut rune::Tx::default());
            if s.devices.this_pc.is_some() || began.elapsed() > Duration::from_secs(10) {
                break s.devices.this_pc.clone();
            }
        };
        assert_eq!(landed.and_then(|d| d.name).as_deref(), Some("slow"));
        poll(&mut s, &mut rune::Tx::default());
        assert!(s.devices.reading.is_none(), "not read again within THIS_PC_EVERY");
    }

    /// The reader's devices as Home says them: what runs on each from what the window holds, its figures, and the mark
    /// only where one needs him: a disk under a tenth free, no answer, at its sensor's own critical heat (docket-edge7
    /// p2814, 3); a field of another type leaves only itself out (here-2's reading).
    #[test]
    fn the_readers_devices_say_what_runs_where_and_mark_only_what_needs_him() {
        let shown = read(Ok(answer()));
        let Shown::Answer(a, age) = &shown else { panic!("{shown:?}") };
        assert_eq!((a.devices.len(), *age), (3, Some(4)));
        assert_eq!((a.devices[2].temp_c, a.devices[2].on), (None, Some(false)), "the heat of another type left out, and nothing else");
        let d = Devices { shown: shown.clone(), this_pc: Some(me()), ..Devices::default() };
        let v = listed(&d);
        let names: Vec<&str> = v.iter().map(|x| x.name.as_deref().unwrap()).collect();
        assert_eq!(names, [THIS_PC, "WSL", "yaaihh-build-1", "yaaihh-build-2"], "the window's own machine first");
        let here: Vec<String> = v.iter().map(|x| runs_here(x, &held_here())).collect();
        assert_eq!(here, ["your window", "server of research (14 agents), cav-paper", "2 jobs running, 1 waiting", "no jobs · its runner does not run"]);
        // each card's figures, the one that needs him in brackets
        let figs = |x: &Device| card_figures(x).into_iter().map(|f| if f.needs { format!("[{} = {}]", f.name, f.words) } else { format!("{} = {}", f.name, f.words) }).collect::<Vec<_>>().join(" · ");
        assert_eq!(figs(&v[0]), "Memory = 20.4 of 31.7 GB · [Disk C: = 25 GB free of 953]");
        assert_eq!(figs(&v[1]), "CPU = load 3.6 of 16 cores · Memory = 18.8 of 39.2 GB · Disk / = 229 GB free of 688");
        assert_eq!(figs(&v[2]), "CPU = load 9 of 20 cores · Memory = 11 of 20 GB");
        assert_eq!(figs(&v[3]), "", "no answer: no figure, its reach says why");
        assert_eq!(v.iter().map(reach).collect::<Vec<_>>(), [None, Some(("reached".to_string(), false)), Some(("reached".to_string(), false)), Some((format!("no answer since {}", crate::when::at("2026-11-24T16:00:00Z")), true))]);
        assert_eq!(v.iter().map(what_it_is).collect::<Vec<_>>(), ["client", "server", "build machine", "build machine"]);
        assert_eq!(v.iter().map(needs_you).collect::<Vec<_>>(), [true, false, false, true]);
        assert_eq!(card_figures(&v[2]).iter().map(|f| f.frac).collect::<Vec<_>>(), [Some(0.45), Some(0.55)], "load against cores, memory against all of it");
        let disk = card_figures(&v[0]).into_iter().find(|f| f.name == "Disk C:").unwrap();
        assert!((disk.frac.unwrap() - (1.0 - 25.0 / 953.0) as f32).abs() < 1e-4, "a disk's bar is how much of it is used");
        // jobs that name no machine are the engine's own machine's; with none anywhere a machine for jobs says so
        let mine = Held { jobs: BTreeMap::from([(None, (0, 1))]), ..Held::default() };
        assert_eq!((runs_here(&v[1], &mine), runs_here(&v[2], &mine)), ("1 job waiting".to_string(), "no jobs".to_string()));
        // too hot at its sensor's own critical heat, no number of ours; under it said, not marked
        let sensor = |n: &str, c: f64, crit: Option<f64>| Temp { name: Some(n.to_string()), c: Some(c), crit_c: crit };
        let hot = Device { on: Some(true), temp_c: Some(96.0), temp_crit_c: Some(95.0), temps: vec![sensor("k10temp Tctl", 96.0, Some(95.0))], ..Device::default() };
        assert!(needs_you(&hot) && card_figures(&hot) == [Figure { name: "Temperature".into(), words: "96 °C k10temp Tctl".into(), frac: Some(1.0), needs: true }]);
        // the hottest on its line, against its own critical heat; the others behind the press (p3125)
        let warm = Device { on: Some(true), temp_c: Some(61.5), temp_crit_c: Some(95.0), temps: vec![sensor("k10temp Tctl", 61.5, Some(95.0)), sensor("acpitz", 45.0, None), sensor("nvme Composite", 40.0, Some(84.0))], ..Device::default() };
        let t = card_figures(&warm).pop().unwrap();
        assert_eq!((t.words.as_str(), t.needs), ("61.5 °C k10temp Tctl", false));
        assert!((t.frac.unwrap() - 61.5 / 95.0).abs() < 1e-4);
        assert_eq!(hardware(&warm), [("Sensors".to_string(), "61.5 °C k10temp Tctl · 45 °C acpitz · 40 °C nvme Composite (critical at 95 °C)".to_string())]);
        // a sample with no `temps` (today's sampler): no temperature row, though it gives its hottest (p3125)
        assert!(card_figures(&Device { temps: vec![], ..warm.clone() }).iter().all(|f| f.name != "Temperature"));
        assert!(!needs_you(&Device { temp_c: Some(58.0), ..hot.clone() }));
        // nothing from the reader: no row, not even the window's own
        assert!(listed(&Devices { this_pc: Some(me()), ..Devices::default() }).is_empty());
        assert_eq!(read(Ok(json!({"text": null, "why": "no reading yet"}).to_string())), Shown::None("no reading yet".into()));
        // a reading a minute old says when it was read; a fresh one says nothing of it
        assert_eq!(as_of(a, Some(60)), Some(format!("as of {}", crate::when::at("2026-11-24T17:01:02Z"))));
        assert_eq!(as_of(a, Some(59)), None);
    }

    /// A device's hardware (CPU, graphics card, memory, each disk's room, sensors) and what else is said of it, each
    /// only when read.
    #[test]
    fn a_devices_hardware_and_what_else_is_said() {
        let Shown::Answer(a, _) = read(Ok(answer())) else { panic!() };
        let pairs = |v: &[(&str, &str)]| v.iter().map(|(l, w)| (l.to_string(), w.to_string())).collect::<Vec<_>>();
        let asked = crate::when::at("2026-11-24T17:01:02Z");
        assert_eq!(hardware(&a.devices[0]), pairs(&[("CPU", "AMD Ryzen 9 7950X 16-Core Processor · 16 cores"), ("Memory", "39.2 GB"), ("Disk /", "229 GB free of 688")]));
        assert_eq!(about(&a.devices[0]), pairs(&[("Ours", "engine research runs · usage reader does not run"), ("Asked", &asked)]));
        assert_eq!((hardware(&a.devices[1]), about(&a.devices[1])), (pairs(&[("CPU", "20 cores"), ("Memory", "20 GB")]), pairs(&[("Runs under", "kvm"), ("Ours", "runner runs")])));
        assert_eq!(hardware(&me()), pairs(&[("Memory", "31.7 GB"), ("Disk C:", "25 GB free of 953")]));
        let gpu = Device { gpu: Some(Gpu { name: Some("RTX 4060".into()), busy_pct: Some(12.0), mem_used_gb: Some(2.0), mem_total_gb: Some(8.0), ..Gpu::default() }), ..Device::default() };
        assert_eq!(hardware(&gpu), pairs(&[("Graphics", "RTX 4060 · 8 GB, 2 used · busy 12%")]));
        assert!(hardware(&a.devices[2]).is_empty() && about(&a.devices[2]).is_empty());
    }
}
