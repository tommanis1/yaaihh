//! coordd: the one live watcher of a box (vault designs/yaaihh-2026-11-07.md §7). It owns the box's agents (process,
//! turn, waits, items), their mail, the project's items and rights, and the project's view sources, and tells every
//! change as a numbered event (`coord watch`). Started with the box: in a docker box as its agent user, in the leaf
//! `coordd` of a cgroup tree handed to that user (box.sh coordd_up_agent), in an sbx box as root; its socket is
//! /run/coordd.sock, and a caller is known by its process's cgroup, never by a name it gives. A project without a box
//! runs it on the host as the owner's user instead (bin/host-coordd: a systemd user unit with a delegated cgroup).
//!
//!   coordd            run (COORD: the project's coord folder; AGENT_BASE: the agent-base checkout; COORDD_SOCK: the
//!                     socket, default /run/coordd.sock; COORDD_CGROUP: the cgroup it owns, default
//!                     /sys/fs/cgroup/swarm; COORDD_LIVE: the live journal, default /run/coordd/live.json)

// the native path (N-412 ask 12): not yet called from the daemon's own paths while its parts are built
#[allow(dead_code)]
mod accounts;
mod cgroup;
mod clean;
mod cold;
mod raw_file;
mod compute_status;
mod split;
mod change;
mod clerk;
mod core;
mod counts;
mod files;
mod find;
mod follow;
mod hook_quick;
mod fork;
mod form;
mod footprint;
mod found;
mod homes;
mod items;
mod jobs;
mod jsonl;
mod keyfile;
mod label;
mod joint;
mod lapse;
mod layer;
mod settings;
mod make;
mod models;
#[cfg(test)]
#[path = "tests/models.rs"]
mod models_tests;
#[cfg(test)]
#[path = "tests/model_guesses.rs"]
mod model_guesses_tests;
#[cfg(test)]
#[path = "tests/labels.rs"]
mod labels_tests;
#[cfg(test)]
#[path = "tests/moment.rs"]
mod moment_tests;
#[cfg(test)]
#[path = "tests/kind_docs.rs"]
mod kind_docs_tests;
#[cfg(test)]
#[path = "tests/raw_record.rs"]
mod raw_record_tests;
#[cfg(test)]
#[path = "tests/agents_given.rs"]
mod agents_given_tests;
#[cfg(test)]
#[path = "tests/names_formed.rs"]
mod names_formed_tests;
#[cfg(test)]
#[path = "tests/given_render.rs"]
mod given_render_tests;
#[cfg(test)]
#[path = "tests/declared.rs"]
mod declared_tests;
#[cfg(test)]
#[path = "tests/slice_record.rs"]
mod slice_record_tests;
#[cfg(test)]
#[path = "tests/handover.rs"]
mod handover_tests;
#[cfg(test)]
#[path = "tests/stop_render.rs"]
mod stop_render_tests;
#[cfg(test)]
#[path = "tests/bare_refs.rs"]
mod bare_refs_tests;
#[cfg(test)]
#[path = "tests/library_text.rs"]
mod library_text_tests;
#[cfg(test)]
#[path = "tests/memory_text.rs"]
mod memory_text_tests;
mod moves;
mod one_call;
mod server_reads;
mod sandbox;
mod schedule;
mod standing;
mod codeview;
mod policy;
mod project;
mod pull;
mod read_ask;
#[cfg(test)]
#[path = "read_ask_flow_tests.rs"]
mod read_ask_flow_tests;
mod render;
mod restart;
mod runner;
mod shape;
mod switch;
mod ticket;
mod transcript;
mod warm;

use crate::cgroup::Caller;
use crate::core::{err, ok, Core, HookNext, Job, Who};
use crate::transcript::{ByModel, Ended, TranscriptUsage};
use coord_types::{Event, ModelUsage, Reply, Request, SOCKET};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::io::unix::AsyncFd;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

/// The core behind its lock; each wait for it is counted for what waits (`counts`, paper-research N-101 p1797). And the
/// places the record gives a text at, the core's own index (`Tickets::places`), which `Request::Text` reads off the lock
/// (paper-research N-1446 p1419, 2).
#[derive(Clone)]
struct Shared(Arc<Mutex<Core>>, Arc<std::sync::RwLock<ticket::text_places::TextPlaces>>);

impl Shared {
    /// CORE behind its lock, with its places index.
    fn new(core: Core) -> Shared {
        let places = Arc::clone(&core.tickets.places);
        Shared(Arc::new(Mutex::new(core)), places)
    }

    #[track_caller]
    fn lock(&self) -> impl std::future::Future<Output = counts::Held<'_, Core>> {
        counts::lock(&self.0)
    }
}

impl std::ops::Deref for Shared {
    type Target = Arc<Mutex<Core>>;
    fn deref(&self) -> &Arc<Mutex<Core>> {
        &self.0
    }
}

/// The agent-base this coordd reads: the one beside its own binary when it is a release's (releases/NAME/coordd and
/// releases/NAME/agent-base, release.sh build), whatever AGENT_BASE says; else AGENT_BASE. A coordd unit restarted
/// as it was keeps the AGENT_BASE it was first made with (host-coordd: a stopped unit whose agents still run stays
/// loaded, and starts again with its old environment), so a release's coordd must not trust it (paper-research m802).
fn agent_base() -> String {
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.canonicalize().ok()?.parent()?.join("agent-base"))).filter(|d| d.join("kinds").is_dir());
    match beside {
        Some(d) => d.display().to_string(),
        None => std::env::var("AGENT_BASE").expect("AGENT_BASE is not set"),
    }
}

/// COORD/agent-base made a link to BASE (this coordd's agent-base), replaced at once (a new link renamed over it), and
/// its path. The agents get it as AGENT_BASE: a release's folder given them, as it was, stayed theirs when an install
/// switched their coordd and left them running, so fresh's session called the old release's coordc, which did not
/// know `--renderings`, and stopped on its help (yaaihh N-1804 p2, 25 Nov). A boxed session sees what the link led to
/// when its box was made (box-enter binds it), until its next start.
fn point_base(coord: &std::path::Path, base: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let link = coord.join("agent-base");
    if std::fs::symlink_metadata(&link).is_ok_and(|m| !m.file_type().is_symlink()) {
        return Err(std::io::Error::other("something not a link is there"));
    }
    if std::fs::read_link(&link).is_ok_and(|to| to == base) {
        return Ok(link);
    }
    let new = coord.join(format!(".agent-base.{}", std::process::id()));
    let _ = std::fs::remove_file(&new);
    std::os::unix::fs::symlink(base, &new)?;
    std::fs::rename(&new, &link)?;
    Ok(link)
}

/// `coordd save-replay COORD [SEED...]` (I-1509): the start's replay of COORD's record saved at its last input but one, read
/// from the record's file alone while the coordd that writes it goes on (`ticket::replay_state::save_on`), with the
/// agent-base, the project and the first ticket number a start of COORD would read. Behind the replay lock, one replay
/// of a record at a time on this machine (`replay_state::save_locked`). At the lowest priority, its
/// memory bounded ([`SAVER_MEMORY`]): one that would need more fails, and the state saved before stays. What it did, on
/// stdout, one line (`saved at line N ...`); its exit code 0, else 1 with why on stderr. SEEDS: states this program saved
/// on copies of the record, the gate's, gone on from when one is this key's and reaches the record (p2658).
fn save_replay(coord: &str, seeds: &[String]) -> i32 {
    // SAFETY: one thread so far; a priority and a limit of this process alone
    unsafe {
        libc::nice(19);
        let cap = SAVER_MEMORY as libc::rlim_t;
        libc::setrlimit(libc::RLIMIT_AS, &libc::rlimit { rlim_cur: cap, rlim_max: cap });
    }
    let base = agent_base();
    let project = match project::Project::load(coord) {
        Ok(p) => p.map(|p| p.dir),
        Err(e) => {
            eprintln!("coordd save-replay: {e}");
            return 1;
        }
    };
    // behind the replay lock, waited for as long as another replay holds it (paper-research N-1446 p1638): the coordd
    // that runs this goes on serving meanwhile, at a switch the old one
    let lock = ticket::replay_state::replay_lock();
    match ticket::replay_state::save_locked(&lock, coord, &base, project.as_deref(), core::first_ticket_of(coord), seeds) {
        Ok(said) => {
            println!("{said}");
            0
        }
        Err(e) => {
            eprintln!("coordd save-replay: {e}");
            1
        }
    }
}

/// The address space a `coordd save-replay` may take (RLIMIT_AS), so that one that would need more fails rather than
/// pressing on the machine. On research's copy (162,901 lines, 22 Nov) one took a VmPeak of 9.9 GB whole and from a
/// state alike (VmHWM 9.7 GB); 14 GB, the laptop's cap on any replay of the record (plan.md), leaves it room to grow.
const SAVER_MEMORY: u64 = 14 << 30;

/// How many lines the record grows past the newest state saved before coordd saves it again (`save_replays`). On
/// research's copy a start went on from a state with a tail of 5,000 lines in 56 s (0 lines: 23 s; 40,000: 493 s;
/// whole: 600 s), and the saver took 92 s from the state before; at 24-40k lines a day that is 5-8 saves a day.
const SAVE_EVERY: usize = 5000;

/// The start's replay saved again as the record grows (I-1509): each minute, once the record has grown [`SAVE_EVERY`]
/// lines past the newest state saved, this program is run again as `coordd save-replay COORD`, one at a time, off the
/// lock, and what it says goes to the journal. A state it saved is where the next start's replay starts from.
fn save_replays(shared: Shared) {
    tokio::spawn(async move {
        let exe = "/proc/self/exe";
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let (coord, lines, saved) = {
                let c = shared.lock().await;
                (c.coord.clone(), c.tickets.record_lines(), c.tickets.saved_at)
            };
            if lines < saved.unwrap_or(0) + SAVE_EVERY {
                continue;
            }
            let began = std::time::Instant::now();
            let run = tokio::task::spawn_blocking(move || std::process::Command::new(exe).args(["save-replay", &coord]).stdin(std::process::Stdio::null()).output()).await;
            let out = match run {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => {
                    eprintln!("coordd: replay state: {exe} save-replay: {e}");
                    continue;
                }
                Err(_) => return,
            };
            let said = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() {
                eprintln!("coordd: replay state: {said} ({:.0} s in all)", began.elapsed().as_secs_f64());
                if let Some(n) = said.strip_prefix("saved at line ").and_then(|s| s.split_whitespace().next()).and_then(|n| n.parse().ok()) {
                    shared.lock().await.tickets.saved_at = Some(n);
                }
            } else {
                // said once a minute at most, the state saved before kept; it is tried again at the next
                eprintln!("coordd: replay state: not saved ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim());
            }
        }
    });
}

/// The folder of coordd's own release (releases/NAME/coordd, release.sh build), the `current` link read through, beside
/// the other releases' folders, each listing the earlier ones it contains (I-1457); none for a build run where it was
/// built.
fn own_release() -> Option<std::path::PathBuf> {
    let dir = std::env::current_exe().ok()?.canonicalize().ok()?.parent()?.to_path_buf();
    (dir.parent()?.file_name()? == "releases").then_some(dir)
}

/// The open descriptors coordd may hold, where its hard limit allows. Each connection is one until it is answered: at
/// 22:48 20 Nov 1101 of a window's requests waited on the lock and took all of the unit's 1024, so the record log
/// could not be opened, and it took no more lines until a restart (N-1396). The programs coordd starts get it too.
const OPEN_FILES: u64 = 16384;

/// The soft limit to set, the current one being CUR and the hard one MAX: [`OPEN_FILES`], or MAX if lower; none if
/// CUR is that already or more.
fn open_files_raised(cur: u64, max: u64) -> Option<u64> {
    let want = OPEN_FILES.min(max);
    (cur < want).then_some(want)
}

fn main() {
    // what this process was started with, before anything in it is changed: a start in place of a coordd whose record
    // failed says so (restart.rs)
    restart::at_start();
    // the allocator's arena limit is coordd's own (paper-research N-412 p1613): glibc read it at the process's start, so
    // coordd runs with it, and it is taken out here, one thread so far, so that nothing coordd starts inherits it
    if std::env::var_os("MALLOC_ARENA_MAX").is_some() {
        unsafe { std::env::remove_var("MALLOC_ARENA_MAX") };
    }
    // glibc's mmap threshold fixed at its default, 128 KB (paper-research N-1446 p2307). Left to itself it rises to the
    // size of the largest mapped block freed so far (a transcript page's window, up to 32 MB), and the trim threshold to
    // twice that: later blocks of that size come from the worker threads' arenas, and freed they stay as each arena's
    // top, which neither free nor malloc_trim gives back (malloc_trim trims only the main arena's top). A reconnect's
    // pages left 200 to 370 MB free in 34 arenas, and research's coordd kept 1.2 GB free overnight; fixed, the same
    // pages leave 4 MB (N-1446 p2305). Fixed, a block of 128 KB or more is mapped by itself and unmapped when freed.
    // Set here, before coordd's threads start and before it allocates much.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallopt only sets the allocator's parameters
        unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 128 * 1024) };
    }
    // but one: `coordd save-replay COORD [SEED...]` saves the start's replay of COORD's record at a cut and ends (I-1509,
    // ticket/replay_state.rs). It starts nothing and takes no lock; COORD is named, never taken from the environment
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [verb, coord, seeds @ ..] = args.as_slice()
        && verb == "save-replay"
    {
        std::process::exit(save_replay(coord, seeds));
    }
    // and `coordd call COORD BASE MODEL`: the prompt on stdin once to MODEL where COORD's policy places it (one_call.rs)
    if let [verb, coord, base, model] = args.as_slice()
        && verb == "call"
    {
        std::process::exit(one_call::run(coord, base, model));
    }
    // coordd takes no arguments: any (even --help) is refused before COORD is read, so a hand-run check can never start
    // it on whatever coord the caller's environment names (paper-research m843)
    if std::env::args().len() > 1 {
        eprintln!("usage: coordd (no arguments; started by host-coordd or box.sh with COORD, COORDD_SOCK and AGENT_BASE set)");
        std::process::exit(2);
    }
    // its limit on open descriptors, raised before any thread starts (N-1396, 22:48 20 Nov)
    let mut lim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: one rlimit, read then set, one thread so far
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } == 0
        && let Some(want) = open_files_raised(lim.rlim_cur as u64, lim.rlim_max as u64)
    {
        let before = lim.rlim_cur;
        lim.rlim_cur = want as libc::rlim_t;
        match unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) } {
            0 => eprintln!("coordd: open descriptors {before} → {want}"),
            _ => eprintln!("coordd: open descriptors left at {before}: {}", std::io::Error::last_os_error()),
        }
    }
    let base = agent_base();
    // what the agents and programs coordd starts are given: $COORD/agent-base, a link to this coordd's own agent-base,
    // so a session that outlives a release (its coordd switched under it) calls the client, roster and hooks of the
    // coordd it talks to; the release's own folder when the link cannot be made
    let given = std::env::var("COORD").ok().and_then(|coord| match point_base(std::path::Path::new(&coord), std::path::Path::new(&base)) {
        Ok(link) => Some(link.display().to_string()),
        Err(e) => {
            eprintln!("coordd: {coord}/agent-base not made ({e}): the agents are given {base}");
            None
        }
    });
    let given = given.unwrap_or_else(|| base.clone());
    if std::env::var("AGENT_BASE").ok().as_deref() != Some(given.as_str()) {
        eprintln!("coordd: agent-base {base}, given as {given} (AGENT_BASE said {})", std::env::var("AGENT_BASE").unwrap_or_default());
        // the agents and programs coordd starts read it too, and find its tools first on PATH (the unit's PATH names
        // releases/current/agent-base/bin, which may move on); one thread so far, before the runtime starts
        let path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("AGENT_BASE", &given);
            std::env::set_var("PATH", format!("{given}/bin:{path}"));
        }
    }
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("the runtime").block_on(run(base));
}

async fn run(base: String) {
    let coord = std::env::var("COORD").expect("COORD is not set");
    let sock = std::env::var("COORDD_SOCK").unwrap_or_else(|_| SOCKET.into());
    if let Err(e) = cgroup::setup() {
        eprintln!("coordd: cgroups at {}: {e}", cgroup::root().display());
        std::process::exit(1);
    }
    let mut core = Core::load(&coord, &base);
    // who holds an account's lock, in the lock's file
    accounts::i_am(&core.coord_id());
    jsonl::start_writer();
    match project::Project::load(&coord) {
        Ok(p) => core.project = p,
        Err(e) => eprintln!("coordd: {e}"),
    }
    core.resolve_modes();
    // the project's own limits on all it runs, and its toolchain first on its agents' PATH (roster reads $COORD/path)
    if let Some(p) = &core.project {
        cgroup::limit_whole(&p.whole_limits());
    }
    // a boxed project's mark for roster: the owner's roster add and restart go through coordd (`coord start`)
    let bf = format!("{coord}/boxed");
    let marked = if core.project.as_ref().is_some_and(|p| p.project.boxed) { std::fs::write(&bf, "") } else { std::fs::remove_file(&bf).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }) };
    if let Err(e) = marked {
        eprintln!("coordd: {bf}: {e}");
    }
    let path = core.project.as_ref().map(|p| p.agent_path()).unwrap_or_default();
    let pf = format!("{coord}/path");
    let written = if path.is_empty() { std::fs::remove_file(&pf).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }) } else { std::fs::write(&pf, format!("{}\n", path.join(":"))) };
    if let Err(e) = written {
        eprintln!("coordd: {pf}: {e}");
    }
    let recovered = core.recover();
    core.load_tickets();
    // who is the runner and each agent's role, as the record's own lines (runner.rs; N-101 post 27, F)
    core.record_roles();
    core.import_items();
    core.hand_over_running(&recovered);
    // each group's processes killed at its memory limit before this start: not told again (`Core::oom_round`); read
    // before the limits are written, so a process the new limit kills now is told (paper-research N-101 p1788)
    core.oom_seen = Core::oom_counts();
    // each agent's limits on its cgroup, once what recover found is in place (it removes the empty ones)
    for (name, l) in start_limits(&core) {
        if let Err(e) = sandbox::limit(&name, &l) {
            eprintln!("coordd: limits of {name}: {e}");
        }
    }
    let views: Vec<(String, String, String)> = core
        .project
        .as_ref()
        .map(|p| p.views.iter().filter_map(|v| v.source.clone().map(|s| (v.id.clone(), p.dir.clone(), s))).collect())
        .unwrap_or_default();
    let events = core.project.as_ref().and_then(|p| Some((p.dir.clone(), p.project.events.clone()?)));
    // the project's shared language servers (`[[lsp]] serve`; N-101 posts 145 and 156)
    let servers: Vec<(String, project::Lsp)> = core.project.as_ref().map(|p| p.lsps.iter().filter(|l| !l.serve.is_empty()).map(|l| (p.dir.clone(), l.clone())).collect()).unwrap_or_default();
    // the cells made once from the month files kept before them, whatever accounts the agents run on
    let seeded = core.coord.clone();
    tokio::task::spawn_blocking(move || match accounts::cells::seed(&seeded) {
        Ok(0) => {}
        Ok(n) => eprintln!("coordd: usage: {n} cells made from the month files"),
        Err(e) => eprintln!("coordd: usage: no cells made from the month files: {e}"),
    });
    // the agents that run on an account send their telemetry here (accounts/telemetry.rs)
    if core.agents.values().any(|a| a.native) {
        telemetry_up(&mut core).await;
    }
    let shared: Shared = Shared::new(core);
    counts::every_minute();
    // a record log that fails: coordd starts again in place, as a coord-switch would, the lock held so nothing more is
    // taken meanwhile (paper-research N-1396, restart.rs); when that is not done, its notice raised at once, not only at
    // the next input refused (paper-research N-8)
    {
        let (s, rt, sock) = (shared.clone(), tokio::runtime::Handle::current(), sock.clone());
        jsonl::on_fault(move |why| {
            let (s, sock) = (s.clone(), sock.clone());
            rt.spawn(async move {
                let mut core = s.lock().await;
                tokio::task::block_in_place(|| restart::again(&sock, &why));
                core.log_fault();
            });
        });
    }
    for (name, pid) in recovered {
        watch_exit(shared.clone(), name, pid);
    }
    // the t3code server of each agy account t3code signed in: one that outlived the last coordd is taken as it is
    tokio::spawn(agy_servers(shared.clone()));
    // his held messages sent once due (schedule.rs)
    // a held message still out being sent when coordd last stopped held again, before any Send can come in
    schedule::found_again(&shared.lock().await.coord.clone()).await;
    tokio::spawn(schedule::watch(shared.clone()));
    // the code view's server let go once no window has been on it a while (codeview::watch), and stopped when coordd
    // is ended by a signal; a start in place (an exec) leaves it running for the new coordd
    {
        let coord = std::path::PathBuf::from(&shared.lock().await.coord);
        let c = coord.clone();
        std::thread::spawn(move || codeview::watch(c));
        for kind in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            let coord = coord.clone();
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::from_raw(kind)) {
                Ok(mut sig) => {
                    tokio::spawn(async move {
                        sig.recv().await;
                        let _ = tokio::task::spawn_blocking(move || codeview::end(&coord)).await;
                        // SAFETY: the signal's own action again, then the signal itself: coordd ends as it would have
                        unsafe {
                            libc::signal(kind, libc::SIG_DFL);
                            libc::raise(kind);
                        }
                    });
                }
                Err(e) => eprintln!("coordd: no handler for signal {kind}: {e}"),
            }
        }
    }
    shared.lock().await.jobs = Some(senders(shared.clone()));
    // the text-change steps given before this start and not answered: run again (change.rs)
    shared.lock().await.text_owed_again();
    // every box of this coord brought up to date: the route's key, written by host-coordd before this start, and the
    // owner's flag cache (sandbox::refresh)
    {
        let coord = shared.lock().await.places().coord.clone();
        let owner = format!("{}/.claude.json", std::env::var("HOME").unwrap_or_default());
        tokio::task::spawn_blocking(move || {
            for e in sandbox::refresh(&coord, &owner) {
                eprintln!("coordd: boxes: {e}");
            }
        });
    }
    ticket::start(shared.0.clone());
    follow(shared.clone());
    count_usage(shared.clone());
    count_footprints(shared.clone());
    read_sides(shared.clone());
    save_replays(shared.clone());
    cold::serve::daily(shared.clone());
    read_models(shared.clone());
    check_agy_at_start(shared.clone());
    read_jobs(shared.clone());
    for (id, dir, cmd) in views {
        let (s, rt) = (shared.clone(), tokio::runtime::Handle::current());
        keep_source(dir, cmd, move |data, error| {
            let (s, id) = (s.clone(), id.clone());
            rt.spawn(counts::timed("source line (a view)", async move { s.lock().await.set_view(&id, data, error) }));
        });
    }
    for (dir, l) in servers {
        keep_server(dir, coord.clone(), l);
    }
    if let Some((dir, cmd)) = events {
        let (s, rt) = (shared.clone(), tokio::runtime::Handle::current());
        keep_source(dir, cmd, move |data, error| {
            let s = s.clone();
            rt.spawn(counts::timed("source line (project events)", project_event(s, data, error)));
        });
    }
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).unwrap_or_else(|e| panic!("coordd: {sock}: {e}"));
    let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o666));
    link_socket(&sock);
    // a start in place's mark taken down, now that a hook that asks is answered, and the next made ready (restart.rs)
    restart::up(&sock);
    eprintln!("coordd: {coord} on {sock}");
    accept(listener, shared).await
}

/// SOCK linked again as SOCK.d/coordd.sock, a folder kept across coordd's restarts: a box binds the folder (agent-base's
/// box-enter), so its agents reach this coordd, not the one before it, whose socket a box that bound the file keeps
/// (paper-research N-101 p1235). A failure is said; the box then binds the socket as before.
pub fn link_socket(sock: &str) {
    let dir = format!("{sock}.d");
    let link = format!("{dir}/coordd.sock");
    let r = std::fs::create_dir_all(&dir)
        .and_then(|_| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)))
        .and_then(|_| match std::fs::remove_file(&link) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        })
        .and_then(|_| std::fs::hard_link(sock, &link));
    if let Err(e) = r {
        eprintln!("coordd: linking {sock} as {link}: {e}; a box started now binds the socket itself, and loses it at coordd's next start");
    }
}

/// Serve each connection to LISTENER for as long as coordd runs. An accept that fails is said once and backed off,
/// never retried at once: out of descriptors, the connection waiting stays waiting and accept fails again, which spun
/// a core and served nobody (docket-try, 12 Nov, 1024 fds).
///
/// Out of descriptors, a client is refused, not left hanging: coordd keeps one descriptor spare for that. The spare
/// is given up only when a connection is actually waiting (the listener polls readable; Linux's accept says EMFILE
/// with nobody waiting too): it is closed and the waiting connection accepted at once, in two system calls one after
/// the other on this thread, nothing awaited between. The connection is shut down both ways, so its client reads
/// the end at once, and that socket is kept as the new spare: no reopening afterwards, in which another thread
/// could take the descriptor (it did: the 100 ms window this replaced, N-27 post 45). Should another thread still
/// take the freed descriptor in the instant between the two calls, the accept fails, and the spare is opened again
/// at the top of each pass until one is free.
async fn accept(listener: UnixListener, shared: Shared) -> ! {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    let spare_file = || std::fs::File::open("/dev/null").ok().map(OwnedFd::from);
    let mut spare: Option<OwnedFd> = spare_file();
    let mut said: Option<std::io::ErrorKind> = None;
    loop {
        if spare.is_none() {
            spare = spare_file();
        }
        match listener.accept().await {
            Ok((stream, _)) => {
                said = None;
                tokio::spawn(serve(shared.clone(), stream));
            }
            Err(e) => {
                if said != Some(e.kind()) {
                    eprintln!("coordd: accept: {e}");
                    said = Some(e.kind());
                }
                let fd = listener.as_raw_fd();
                let mut waiting = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
                // SAFETY: one pollfd for a descriptor LISTENER holds open; a zero timeout, so nothing blocks
                let readable = unsafe { libc::poll(&mut waiting, 1, 0) } == 1 && waiting.revents & libc::POLLIN != 0;
                if matches!(e.raw_os_error(), Some(libc::EMFILE) | Some(libc::ENFILE)) && readable {
                    if let Some(s) = spare.take() {
                        drop(s);
                        // SAFETY: accept4 on the listening socket LISTENER holds; it is non-blocking (tokio's), so
                        // this returns at once, with the new descriptor or -1
                        let got = unsafe { libc::accept4(fd, std::ptr::null_mut(), std::ptr::null_mut(), libc::SOCK_CLOEXEC) };
                        if got >= 0 {
                            // SAFETY: GOT is a descriptor accept4 just gave this process, owned by nothing else
                            let refused = unsafe { OwnedFd::from_raw_fd(got) };
                            // SAFETY: shutdown of the socket REFUSED owns; its client reads the end, its writes fail
                            unsafe { libc::shutdown(refused.as_raw_fd(), libc::SHUT_RDWR) };
                            spare = Some(refused);
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

#[cfg(test)]
mod snapshot_chunk_tests {
    use super::*;
    use docket_machine::{Input, Text};

    fn tickets(e: &Event) -> serde_json::Value {
        match &e.body {
            coord_types::Body::Snapshot(s) => serde_json::to_value(&s.tickets).unwrap(),
            _ => panic!("no snapshot"),
        }
    }

    /// A whole snapshot built a few tickets at a time (paper-research N-560 p29, 2) is the one a single hold gives at
    /// its last hold: a ticket changed after its view was built is built again, from the events the watch holds, and
    /// one opened meanwhile is in it.
    #[test]
    fn a_snapshot_built_a_few_tickets_at_a_time_is_the_one_its_last_hold_would_give() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-chunk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let open = |core: &mut Core, text: &str| {
            let form = serde_json::json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
            let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New(text.into())), names: vec!["w2".into()], told: vec![] }).unwrap();
            docket_machine::parse_ticket(r.trim_start_matches("Opened ").trim_end_matches('.')).unwrap()
        };
        let a = open(&mut core, "first");
        let mut rx = core.tx.subscribe();
        // the views built (as the chunks do), then A gets a post and B is opened before the last hold
        let built: std::collections::BTreeMap<u64, docket_machine::TicketView> = core.ticket_view_ids(|_| true).into_iter().filter_map(|t| Some((t, core.ticket_shown(t)?))).collect();
        core.ticket_input(Input::Post { t: a, by: "owner".into(), text: Text::New("and more".into()), names: vec!["w2".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let b = open(&mut core, "second");
        let snap = snapshot_finished(&core, built.clone(), &mut rx);
        assert_eq!(tickets(&snap), tickets(&core.snapshot()), "as one hold gives it now");
        assert_eq!(snap.seq, core.snapshot().seq);
        let posts = |e: &Event, t: u64| match &e.body { coord_types::Body::Snapshot(s) => s.tickets.iter().find(|v| v.id == core.tickets.m.id(t)).map(|v| v.posts.len()), _ => None };
        assert_eq!((posts(&snap, a), posts(&snap, b).is_some()), (Some(2), true));
        // without the events (a watch that heard nothing) the old view would stand: they are what keeps it whole
        let mut deaf = core.tx.subscribe();
        let stale = snapshot_finished(&core, built, &mut deaf);
        assert_eq!(posts(&stale, a), Some(1));
        let _ = std::fs::remove_dir_all(&coord);
    }
}

#[cfg(test)]
mod record_tests {
    use super::*;
    use docket_machine::{Input, Text};
    use std::os::unix::fs::OpenOptionsExt;

    /// The window's Record layer reads research's 140 MB record off the core lock (consult-3 N-1446 p34): while the
    /// read waits on the file, here a FIFO nobody writes yet, the lock is free and another request is answered; the
    /// lines written then come back as the thread's.
    #[tokio::test]
    async fn the_record_is_read_off_the_lock_while_another_request_is_answered() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-record-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let form = serde_json::json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
        let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(Text::New("the parser".into())), names: vec!["w2".into()], told: vec![] }).unwrap();
        let id = r.trim_start_matches("Opened ").trim_end_matches('.').to_string();
        let t = docket_machine::parse_ticket(&id).unwrap();
        // the record set aside, a FIFO in its place: the read blocks until it is written
        let log = coord.join("tickets/log.jsonl");
        std::fs::rename(&log, coord.join("tickets/log.kept")).unwrap();
        assert!(std::process::Command::new("mkfifo").arg(&log).status().unwrap().success());
        let shared = Shared::new(core);
        let reading = tokio::spawn({
            let (shared, id) = (shared.clone(), id.clone());
            async move { record_off_lock(&shared, &id, None, None, 0).await }
        });
        // the read is at the file once a writer can open it without waiting
        let writer = loop {
            match std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&log) {
                Ok(w) => break w,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(5)).await,
            }
        };
        let answered = tokio::time::timeout(std::time::Duration::from_secs(2), async { shared.lock().await.ticket_show(&id) }).await;
        assert!(answered.is_ok_and(|r| r.ok), "another request answered while the record is read");
        assert!(!reading.is_finished());
        let line = format!("{{\"ev\":\"post\",\"at\":\"2026-11-20T18:23:17Z\",\"t\":{t},\"by\":\"owner\",\"text\":\"like ofc\"}}");
        std::io::Write::write_all(&mut &writer, format!("{line}\n{{\"ev\":\"post\",\"at\":\"x\",\"t\":999}}\n").as_bytes()).unwrap();
        drop(writer);
        let r = reading.await.unwrap();
        assert!(r.ok, "{}", r.out);
        let got: Vec<coord_types::RecordLine> = r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(got, [coord_types::RecordLine { line: 1, text: line }]);
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Insight (1)'s turns are asked through the request coordd answers off the lock (`insight_off_lock`): the owner's
    /// alone, an agent's refused; the record's `turn_ended` lines come back as turns, each with its line's number, an
    /// older line without tools or model among them.
    #[tokio::test]
    async fn insight_turns_are_the_owners_read_off_the_lock() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-insight-turns-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"lead\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let record = core.record_file();
        let before = std::fs::read_to_string(&record).unwrap_or_default().lines().count() as u64;
        let lines = [
            r#"{"ev":"turn_ended","at":"2026-11-20T09:00:00Z","agent":"lead","delivery":"N-1.p2","text":{"file":"t.jsonl","offset":0,"len":5},"stood":""}"#,
            r#"{"ev":"post","at":"2026-11-20T09:00:01Z","t":1,"by":"owner","text":"turn_ended, in words: no turn"}"#,
            r#"{"ev":"turn_ended","at":"2026-11-20T09:05:00Z","agent":"fixer","delivery":"N-1.p3","text":{"file":"t.jsonl","offset":5,"len":5},"stood":"","tools":{"Bash":2},"model":"claude-opus-5-5"}"#,
        ];
        std::io::Write::write_all(&mut std::fs::OpenOptions::new().create(true).append(true).open(&record).unwrap(), (lines.join("\n") + "\n").as_bytes()).unwrap();
        let shared = Shared::new(core);
        let refused = insight_off_lock(&shared, &Who::Agent("lead".into())).await;
        assert!(!refused.ok && refused.out.contains("owner's"), "{}", refused.out);
        let r = insight_off_lock(&shared, &Who::Owner).await;
        assert!(r.ok, "{}", r.out);
        let got: Vec<docket_machine::insight::Turn> = r.out.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let seen: Vec<(&str, u64, Option<&str>, usize)> = got.iter().map(|x| (x.agent.as_str(), x.line, x.model.as_deref(), x.tools.len())).collect();
        assert_eq!(seen, [("lead", before + 1, None, 0), ("fixer", before + 3, Some("claude-opus-5-5"), 1)]);
        let _ = std::fs::remove_dir_all(&coord);
    }
}

#[cfg(test)]
mod text_place_tests {
    use super::*;

    /// A coord in DIR whose record has one post, the owner's thread to w2, its text at LOC: the core, its tickets loaded.
    pub(super) fn posted_at(dir: &std::path::Path, base: &str, loc: &docket_machine::Loc) -> Core {
        std::fs::write(dir.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&dir.display().to_string(), base);
        core.load_tickets();
        let form = serde_json::json!({"to": "owner", "typed": [], "names": [], "text": "", "addressed": {"head": "Sent", "by": "owner"}, "follows": null});
        let r = core.ticket_input(docket_machine::Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form, text: Some(docket_machine::Text::Loc { loc: loc.clone() }), names: vec!["w2".into()], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        core
    }

    async fn text(shared: &Shared, caller: &Caller, loc: &docket_machine::Loc) -> Reply {
        handle(shared, caller, 0, Request::Text { loc: loc.clone() }).await
    }

    /// `Text` reads a place only when a line of the record gives a text at it (paper-research N-1446 p1419, 2). A
    /// post's place is served, to him and to an agent (`coord show T.pN` asks it so), and again after a start, read
    /// from the record. Refused, each saying why and with none of the words: a place made up into another file, one
    /// an agent wrote into an act's `more` (in the record, as a value), another place in the post's own file, the
    /// post's place cut shorter, and a file the record never named.
    #[tokio::test]
    async fn a_text_is_read_only_at_a_place_the_record_gives() {
        let dir = std::env::temp_dir().join(format!("coordd-text-places-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let session = dir.join("w2.jsonl");
        let entry = r#"{"message":{"content":[{"type":"text","text":"the words at their place"}]}}"#;
        std::fs::write(&session, format!("{entry}\n{entry}\n")).unwrap();
        let other = dir.join("other.jsonl");
        let theirs = r#"{"message":{"content":[{"type":"text","text":"words of another file"}]}}"#;
        std::fs::write(&other, format!("{theirs}\n")).unwrap();
        let at = |file: &std::path::Path, offset: u64, len: u64| docket_machine::Loc { file: file.display().to_string(), offset, len };
        let len = entry.len() as u64;
        let loc = at(&session, 0, len);
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut core = posted_at(&dir, &base, &loc);
        let made_up = at(&other, 0, theirs.len() as u64);
        let mut more = serde_json::Map::new();
        more.insert("text".into(), serde_json::json!({"loc": made_up}));
        let r = core.ticket_input(docket_machine::Input::Act { agent: "w2".into(), is: "note".into(), text: String::new(), more, t: None });
        assert!(r.is_ok(), "{r:?}");
        assert!(std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap().contains("other.jsonl"), "the act's value is in the record");
        let refused = |r: Reply, what: &str| {
            assert!(!r.ok && r.out.starts_with("no line of the record gives a text at"), "{what}: {r:?}");
            assert!(!r.out.contains("another file") && !r.out.contains("their place"), "{what}: a refusal holds no words: {r:?}");
        };
        let owner = Caller::Owner;
        let agent = Caller::Agent("w2".into());
        let shared = Shared::new(core);
        for who in [&owner, &agent] {
            let r = text(&shared, who, &loc).await;
            assert!(r.ok && r.out == "the words at their place", "{r:?}");
        }
        for (l, what) in [(made_up.clone(), "made up, another file"), (at(&session, len + 1, len), "another place in its file"), (at(&session, 0, len - 1), "cut shorter"), (at(&dir.join("w3.jsonl"), 0, len), "a file never named")] {
            refused(text(&shared, &owner, &l).await, what);
            refused(text(&shared, &agent, &l).await, what);
        }
        // started again: the places are the record's lines
        drop(shared);
        let mut again = Core::load(&dir.display().to_string(), &base);
        again.load_tickets();
        let shared = Shared::new(again);
        assert_eq!(text(&shared, &owner, &loc).await.out, "the words at their place");
        refused(text(&shared, &owner, &made_up).await, "made up, after a start");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod fd_tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    fn open_fds() -> usize {
        std::fs::read_dir("/proc/self/fd").map(|d| d.count()).unwrap_or(0)
    }

    /// The unit's 1024 under a hard limit of 1048576 (research's coordd, 20 Nov) goes to OPEN_FILES; a lower hard
    /// limit is the most it goes to; a soft limit already as high, or higher, is left as it is.
    #[test]
    fn coordd_raises_its_open_descriptors_to_open_files() {
        assert_eq!(open_files_raised(1024, 1048576), Some(OPEN_FILES));
        assert_eq!(open_files_raised(1024, 4096), Some(4096));
        assert_eq!(open_files_raised(1024, 1024), None);
        assert_eq!(open_files_raised(OPEN_FILES, 1048576), None);
        assert_eq!(open_files_raised(65536, 1048576), None);
    }

    /// The pidfds this process holds now (`anon_inode:[pidfd]` in /proc/self/fd).
    fn pidfds() -> usize {
        std::fs::read_dir("/proc/self/fd")
            .map(|d| d.flatten().filter(|e| std::fs::read_link(e.path()).is_ok_and(|l| l.as_os_str() == "anon_inode:[pidfd]")).count())
            .unwrap_or(0)
    }

    /// A thousand processes end, each watched as an agent's is (`exit_of`, watch_exit's wait): each has its pidfd while
    /// it runs, and none is left once they have ended (paper-research N-1396 p376: 25 pidfds in research's coordd at
    /// 22:48, one for each agent alive). A few may be another test's, run beside this one.
    #[test]
    fn a_thousand_processes_ending_leave_no_pidfd() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let before = pidfds();
        let mut most = before;
        for _ in 0..10 {
            let mut children: Vec<std::process::Child> = (0..100).map(|_| std::process::Command::new("sleep").arg("0.2").spawn().unwrap()).collect();
            let waits: Vec<_> = children.iter().map(|c| rt.spawn(exit_of(c.id() as i32))).collect();
            std::thread::sleep(std::time::Duration::from_millis(50));
            most = most.max(pidfds());
            rt.block_on(async {
                for w in waits {
                    w.await.unwrap();
                }
            });
            for c in &mut children {
                c.wait().unwrap();
            }
        }
        assert!(most >= before + 50, "the processes were watched by their pidfds: {before} before, {most} at most");
        assert!(pidfds() <= before + 5, "no pidfd left after a thousand ends: {before} before, {} after", pidfds());
    }

    /// The two tests below run one at a time: the limit test's parent holds its watches in this process.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The connections coordd on SOCK holds: its accepted sockets, which carry the listener's path in /proc/net/unix
    /// (the listener itself, flagged listening, left out). Only this test's coordd is counted, not the process's sockets.
    fn served(sock: &str) -> usize {
        let t = std::fs::read_to_string("/proc/net/unix").unwrap();
        t.lines().skip(1).filter(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() >= 8 && f[7] == sock && f[3] != "00010000"
        }).count()
    }

    /// A coordd on a socket in a fresh folder: its path, the accept loop running.
    fn coordd(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("coordd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let core = Core::load(&dir.display().to_string(), &base);
        let sock = dir.join("coordd.sock").display().to_string();
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(accept(listener, Shared::new(core)));
        sock
    }

    /// A watch on SOCK that has had its snapshot; None when coordd refused it or said nothing within three seconds.
    async fn watching(sock: &str) -> Option<UnixStream> {
        let mut s = UnixStream::connect(sock).await.ok()?;
        s.write_all(format!("{}\n", serde_json::to_string(&Request::Watch { from: None, run: None, box_id: None }).unwrap()).as_bytes()).await.ok()?;
        let mut b = [0u8; 1];
        match tokio::time::timeout(std::time::Duration::from_secs(3), s.read(&mut b)).await {
            Ok(Ok(1)) => Some(s),
            _ => None,
        }
    }

    /// A thousand texts asked while coordd's lock is held (22:48 20 Nov: a window's 1101 waited 12 s each on it, every
    /// one a connection, until coordd had no descriptor left for its record log): each is answered at once, off the
    /// lock, its socket is given back, and the record still takes lines (consult-3 N-1396 p382). In rounds of 100, so
    /// the test's own ends stay under its limit.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_thousand_texts_asked_under_a_held_lock_are_answered_and_leave_no_socket() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("coordd-texts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // a transcript's entry: the words a post's place points to
        let file = dir.join("session.jsonl");
        let entry = r#"{"message":{"content":[{"type":"text","text":"the words at their place"}]}}"#;
        std::fs::write(&file, format!("{entry}\n")).unwrap();
        let loc = docket_machine::Loc { file: file.display().to_string(), offset: 0, len: entry.len() as u64 };
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let shared = Shared::new(text_place_tests::posted_at(&dir, &base, &loc));
        let sock = dir.join("coordd.sock").display().to_string();
        tokio::spawn(accept(UnixListener::bind(&sock).unwrap(), shared.clone()));
        let line = format!("{}\n", serde_json::to_string(&Request::Text { loc }).unwrap());
        let held = shared.lock().await;
        for _ in 0..10 {
            let asks: Vec<_> = (0..100)
                .map(|_| {
                    let (sock, line) = (sock.clone(), line.clone());
                    tokio::spawn(async move {
                        let mut s = UnixStream::connect(&sock).await.unwrap();
                        s.write_all(line.as_bytes()).await.unwrap();
                        let mut reply = String::new();
                        let read = tokio::time::timeout(std::time::Duration::from_secs(5), BufReader::new(s).read_line(&mut reply)).await;
                        (read.is_ok(), reply)
                    })
                })
                .collect();
            for a in asks {
                let (answered, reply) = a.await.unwrap();
                assert!(answered, "a text waited on the held lock");
                let r: Reply = serde_json::from_str(&reply).unwrap();
                assert!(r.ok && r.out == "the words at their place", "{r:?}");
            }
        }
        drop(held);
        let mut now = served(&sock);
        for _ in 0..250 {
            if now == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            now = served(&sock);
        }
        assert_eq!(now, 0, "coordd still holds {now} of the texts' sockets");
        assert_eq!(shared.lock().await.log_fault(), None, "the record takes lines");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Watches whose clients went give their descriptors back at once, with nothing happening on the coord.
    #[tokio::test]
    async fn a_watch_whose_client_went_lets_its_socket_go() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let sock = coordd("watch-gone");
        let mut held = Vec::new();
        for _ in 0..100 {
            held.push(watching(&sock).await.expect("a snapshot"));
        }
        assert_eq!(served(&sock), 100, "one served socket per watch");
        drop(held);
        let mut now = served(&sock);
        for _ in 0..250 {
            if now == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            now = served(&sock);
        }
        assert_eq!(now, 0, "coordd still holds {now} of the 100 watches' sockets");
    }

    /// The child coordd of the test below: killed when dropped, so a failing test does not leave it serving.
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    /// Out of descriptors, coordd refuses a new connection at once and serves again once some are freed. Run in a
    /// child process (this test binary again) with a low limit, so the test runner's own limit is not touched.
    /// Neither side can outlive a minute (paper-research N-27 post 29: runs of it were left going for hours): the
    /// child dies with its parent and ends itself at its own deadline; the parent fails at its deadline, saying what
    /// it was waiting on, and kills the child as it fails.
    #[test]
    fn at_the_limit_a_connection_is_refused_not_left_hanging() {
        const DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);
        if let Ok(sock) = std::env::var("COORDD_FD_CHILD") {
            // the parent gone (killed, or its runner stopped): the child goes too
            unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) };
            let open = open_fds() as u64;
            let lim = libc::rlimit { rlim_cur: open + 40, rlim_max: open + 40 };
            assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) }, 0);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let dir = std::path::Path::new(&sock).parent().unwrap().display().to_string();
                let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
                let core = Core::load(&dir, &base);
                let listener = UnixListener::bind(&sock).unwrap();
                let _ = tokio::time::timeout(DEADLINE, accept(listener, Shared::new(core))).await;
            });
            std::process::exit(0);
        }
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("coordd-limit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("coordd.sock").display().to_string();
        let _child = Child(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["fd_tests::at_the_limit_a_connection_is_refused_not_left_hanging", "--exact", "--nocapture", "--test-threads=1"])
                .env("COORDD_FD_CHILD", &sock)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let waiting = std::sync::Arc::new(std::sync::Mutex::new("the child's socket"));
        let w = waiting.clone();
        let at = |what: &'static str| *w.lock().unwrap() = what;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let run = rt.block_on(async { tokio::time::timeout(DEADLINE, async {
            for _ in 0..500 {
                if std::path::Path::new(&sock).exists() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            // fill it: watches until one is not served
            at("watches, until one is not served");
            let mut held = Vec::new();
            while let Some(s) = watching(&sock).await {
                held.push(s);
                assert!(held.len() < 100, "no limit reached");
            }
            assert!(held.len() >= 10, "{} watches", held.len());
            // at the limit: a new connection is closed by coordd, not left waiting
            at("the refusal at the limit");
            let t = std::time::Instant::now();
            let mut s = UnixStream::connect(&sock).await.unwrap();
            let _ = s.write_all(b"{\"op\":\"watch\",\"from\":null}\n").await;
            let mut b = [0u8; 1];
            // within seconds, not at the 30 s a silent client is given
            let got = tokio::time::timeout(std::time::Duration::from_secs(5), s.read(&mut b)).await;
            assert!(matches!(got, Ok(Ok(0)) | Ok(Err(_))), "refused: {got:?}");
            assert!(t.elapsed() < std::time::Duration::from_secs(5), "{:?}", t.elapsed());
            // and once watches go, it serves again
            drop(held);
            at("a watch served again");
            let mut back = None;
            for _ in 0..50 {
                back = watching(&sock).await;
                if back.is_some() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            assert!(back.is_some(), "served again");
        }).await });
        let _ = std::fs::remove_dir_all(&dir);
        assert!(run.is_ok(), "not done within {DEADLINE:?}: waiting on {}", waiting.lock().unwrap());
    }
}

#[cfg(test)]
mod account_key_tests {
    use super::*;
    use accounts::{testing::Tmp, Harness, Kind};

    /// Every file under DIR (links not followed) whose bytes hold WORD.
    fn holding(dir: &std::path::Path, word: &str) -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let (path, kind) = (e.path(), e.file_type().unwrap());
            if kind.is_dir() {
                found.extend(holding(&path, word));
            } else if kind.is_file() && String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(word) {
                found.push(path);
            }
        }
        found
    }

    #[tokio::test]
    async fn the_windows_key_goes_to_the_accounts_key_file_and_nowhere_else() {
        let t = Tmp::new("key-store");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        let h = Harness::Claude;
        t.store.create(h, "k", "Work API", 10, Kind::Key, "x").unwrap();
        t.store.create(h, "a", "a", 0, Kind::Signin, "x").unwrap();
        let shared = Shared::new(core);
        let typed = |key: &str| coord_types::Secret::new(key.into());
        let file = t.store.key_file(h, "k");
        // the owner's window only: an agent, and a caller that is nobody (a terminal holder, a project's source)
        for who in [Some(Who::Agent("w".into())), None] {
            let r = account_key_store(&shared, who, "k".into(), typed("made-up-key-7f3")).await;
            assert_eq!(r, err("account k: key not stored: a key is given in the owner's window only".into()));
        }
        // one kind per folder; nothing typed; a key has no spaces. No refusal holds what was typed
        for (id, key, why) in [("none", "made-up-key-7f3", "there is no such account"), ("a", "made-up-key-7f3", "it runs on a sign-in, not on a key"), ("k", " \n", "nothing typed"), ("k", "made-up key-7f3", "a key has no spaces: paste it again")] {
            let r = account_key_store(&shared, Some(Who::Owner), id.into(), typed(key)).await;
            assert_eq!(r, err(format!("account {id}: key not stored: {why}")));
        }
        assert!(!file.exists() && !t.store.state(h, "k").unwrap().signed_in);
        // a word that is no id names no account, though a key account's record lies where it points
        let planted = t.planted();
        let r = account_key_store(&shared, Some(Who::Owner), "../planted".into(), typed("made-up-key-7f3")).await;
        assert_eq!(r, err("account ../planted: key not stored: there is no such account".into()));
        assert!(!planted.join("key").exists() && holding(&t.dir, "made-up-key").is_empty());
        // stored: the file, the owner's alone, a pasted key's last newline not doubled; the account can take agents
        let r = account_key_store(&shared, Some(Who::Owner), "k".into(), typed("made-up-key-7f3\n")).await;
        assert_eq!(r, ok("account k: key stored".into()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "made-up-key-7f3\n");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(t.store.state(h, "k").unwrap().signed_in);
        // it is in that file and in no other: not in the coord's folder (record, registry, state), not elsewhere in
        // the store, and not in what the page is given, which says only that a key is there and since when
        assert_eq!(holding(&t.dir, "made-up-key"), [file.clone()]);
        let page = shared.lock().await.accounts_listing().reply(true).out;
        assert!(!page.contains("made-up-key"), "{page}");
        let page: coord_types::NativeAccounts = serde_json::from_str(&page).unwrap();
        let k = page.accounts.iter().find(|a| a.id == "k").unwrap();
        assert!(k.key && k.signed_in && k.key_since.is_some_and(|at| (accounts::mover::now_s() - at).abs() < 60), "{k:?}");
        assert_eq!(page.accounts.iter().find(|a| a.id == "a").unwrap().key_since, None);
        // a new key replaces it
        assert!(account_key_store(&shared, Some(Who::Owner), "k".into(), typed("made-up-other-91c")).await.ok);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "made-up-other-91c\n");
        assert_eq!(holding(&t.dir, "made-up-"), [file]);
        // whatever prints the request for debugging prints no key; on the wire it is there, as it must be
        let req = Request::AccountKeyStore { id: "k".into(), key: typed("made-up-key-7f3") };
        assert_eq!(format!("{req:?}"), "AccountKeyStore { id: \"k\", key: Secret(…) }");
        assert_eq!(serde_json::to_string(&req).unwrap(), r#"{"op":"accountkeystore","id":"k","key":"made-up-key-7f3"}"#);
    }

    #[tokio::test]
    async fn an_opencode_account_is_not_removed_under_a_running_agent_and_a_binding_nobody_runs_is_let_go() {
        let t = Tmp::new("oc-remove");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        let add = |name: &str, n: u32| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{n}\",\"name\":\"{name}\",\"parent\":\"owner\",\"kind\":\"opencode\",\"model\":\"opencode-go/kimi-k2\",\"at\":\"x\"}}\n");
        std::fs::write(coord.join("registry.jsonl"), add("w", 1) + &add("v", 2)).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        let h = Harness::Opencode;
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        // w runs on it; v was bound and has stopped; gone was an agent of this coord that is no more
        for name in ["w", "v", "gone"] {
            t.store.bind(h, "go", "research", name, "x").unwrap();
        }
        core.agents.get_mut("w").unwrap().live = crate::core::Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        let shared = Shared::new(core);
        let r = account_remove(&shared, &Who::Owner, "go", false).await;
        assert_eq!(r, err("coord account remove: research.w runs on it; it can be removed once it is stopped".into()));
        // nothing marked, the account stays, and only the running agent's binding is left
        let e = t.store.get(h, "go").unwrap();
        assert!(e.state.leaving.is_none(), "not being removed");
        assert_eq!(e.bound.iter().map(|b| b.agent.as_str()).collect::<Vec<_>>(), ["w"]);
    }

    /// The card's `Rename` and `Turn off` · `Turn on` (N-1437), one request with the tier's: an account turned off
    /// takes no agent and its agent is marked to move at the end of its turn, as a removed one's is, but it stays,
    /// and once on it takes agents again.
    #[tokio::test]
    async fn an_edit_renames_turns_off_and_on_and_a_tier_is_its_edit_alone() {
        let t = Tmp::new("edit");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"claude-opus-5-5\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        let h = Harness::Claude;
        for (id, tier) in [("a", 10), ("b", 0)] {
            t.store.create(h, id, id, tier, Kind::Signin, "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.heard) = (true, Some(1))).unwrap();
        }
        // w runs natively on a, not in a turn
        let w = core.agents.get_mut("w").unwrap();
        w.native = true;
        w.live = crate::core::Live { pid: Some(std::process::id() as i32), state: "idle".into(), ..Default::default() };
        t.store.bind(h, "a", "research", "w", "x").unwrap();
        let shared = Shared::new(core);
        let edit = |id: &str, tier: Option<i64>, label: Option<&str>, off: Option<bool>| Request::AccountEdit { id: id.into(), tier, label: label.map(str::to_string), off };
        let now = accounts::mover::now_s;
        // the owner's only
        assert_eq!(account_store(&shared, &Who::Agent("w".into()), edit("a", None, Some("Work"), None)).await, err("coord account edit: the owner's".into()));
        // a rename, its spaces trimmed; a label that is not words on one line is refused, and nothing changes
        assert_eq!(account_store(&shared, &Who::Owner, edit("a", None, Some("  Work (Max) "), None)).await, ok("account a: label “Work (Max)”".into()));
        for bad in ["  ", "two\nlines"] {
            assert_eq!(account_store(&shared, &Who::Owner, edit("a", None, Some(bad), None)).await, err("coord account edit: a label is words on one line".into()));
        }
        assert_eq!(t.store.get(h, "a").unwrap().account.label, "Work (Max)");
        // `coord account tier` is the edit of a tier alone, in its own words
        assert_eq!(account_store(&shared, &Who::Owner, Request::AccountTier { id: "a".into(), tier: 20 }).await, ok("account a: tier 20".into()));
        assert_eq!(t.store.get(h, "a").unwrap().account.tier, 20);
        // off: it takes no agent; w is marked now to move at the end of its turn; the page says since when
        let r = account_store(&shared, &Who::Owner, edit("a", None, None, Some(true))).await;
        assert_eq!(r, ok("account a: off: it takes no agent; its agents move at the end of their turn".into()));
        let since = t.store.state(h, "a").unwrap().off.unwrap();
        assert!(!t.store.state(h, "a").unwrap().can_take(now()) && (now() - since).abs() < 60);
        assert_eq!(shared.lock().await.agents["w"].live.account_due.as_deref(), Some("its account is turned off"));
        assert_eq!(accounts::mover::due(&t.store.state(h, "a").unwrap(), now()).as_deref(), Some("its account is turned off"));
        let page: coord_types::NativeAccounts = serde_json::from_str(&shared.lock().await.accounts_listing().reply(true).out).unwrap();
        assert_eq!(page.accounts.iter().find(|x| x.id == "a").unwrap().off, Some(since));
        assert_eq!(page.accounts.iter().find(|x| x.id == "b").unwrap().off, None);
        // turned off again it keeps its first time; once nobody is bound to it, the round removes nothing
        assert!(account_store(&shared, &Who::Owner, edit("a", None, None, Some(true))).await.ok);
        assert_eq!(t.store.state(h, "a").unwrap().off, Some(since));
        t.store.unbind_from(h, "a", "research", "w");
        let seen = accounts::mover::seen(&t.store);
        assert!(seen.off.is_empty() && seen.leaving.is_empty());
        assert!(shared.lock().await.accounts_seen(&seen).is_empty(), "an account turned off is not removed");
        assert!(t.store.get(h, "a").is_some());
        // on: it takes agents again
        assert_eq!(account_store(&shared, &Who::Owner, edit("a", None, None, Some(false))).await, ok("account a: on: it takes agents again".into()));
        assert!(t.store.state(h, "a").unwrap().off.is_none() && t.store.state(h, "a").unwrap().can_take(now()));
        // all three at once, each said; an account that is not there is refused
        let r = account_store(&shared, &Who::Owner, edit("b", Some(5), Some("Spare"), Some(true))).await;
        assert_eq!(r, ok("account b: tier 5, label “Spare”, off: it takes no agent; its agents move at the end of their turn".into()));
        let b = t.store.get(h, "b").unwrap();
        assert!((b.account.tier, b.account.label.as_str(), b.state.off.is_some()) == (5, "Spare", true));
        assert_eq!(account_store(&shared, &Who::Owner, edit("none", None, None, Some(true))).await, err("coord account edit: no account none".into()));
        // an edit that changes nothing is refused, and nothing is written
        let before = t.store.get(h, "b").unwrap();
        assert_eq!(account_store(&shared, &Who::Owner, edit("b", None, None, None)).await, err("coord account edit: nothing to change".into()));
        assert_eq!(t.store.get(h, "b").unwrap(), before);
    }

    /// An agent marked in its turn to move off its account, because the owner turned the account off or was removing
    /// it, stays on it when the account is turned on, or kept, before that turn ends: no restart, nothing said of a
    /// move (consult-3, N-1437 p7). One whose account is still off when its turn ends moves.
    #[tokio::test]
    async fn turned_on_or_kept_before_its_agents_turn_ends_it_stays_on_its_account() {
        let t = Tmp::new("on-in-turn");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"claude-opus-5-5\",\"at\":\"x\"}\n").unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        let h = Harness::Claude;
        for (id, tier) in [("a", 10), ("b", 0)] {
            t.store.create(h, id, id, tier, Kind::Signin, "x").unwrap();
            t.store.update(h, id, |s| (s.signed_in, s.heard) = (true, Some(1))).unwrap();
        }
        // w runs natively on a, in a turn
        let w = core.agents.get_mut("w").unwrap();
        (w.native, w.account, w.ended_turn) = (true, Some("a".into()), 0);
        w.live = crate::core::Live { pid: Some(std::process::id() as i32), state: "working".into(), turn: 1, ..Default::default() };
        t.store.bind(h, "a", "research", "w", "x").unwrap();
        let shared = Shared::new(core);
        let turned = |off: bool| Request::AccountEdit { id: "a".into(), tier: None, label: None, off: Some(off) };
        let turn_begins = || async {
            let mut c = shared.lock().await;
            let w = c.agents.get_mut("w").unwrap();
            (w.live.state, w.live.turn) = ("working".into(), w.live.turn + 1);
        };
        // its turn ends: what is restarted then
        let turn_ends = || async {
            let mut c = shared.lock().await;
            let w = c.agents.get_mut("w").unwrap();
            (w.live.state, w.ended_turn) = ("idle".into(), w.live.turn);
            c.account_turn_ended("w");
            c.to_restart()
        };
        let stayed = || async {
            let c = shared.lock().await;
            let w = &c.agents["w"];
            assert!(w.live.account_due.is_none() && c.account_note(w).is_none(), "{:?}", c.account_note(w));
            assert_eq!(t.store.bound_to(h, "research", "w").map(|e| e.id).as_deref(), Some("a"));
        };
        // turned off in its turn: marked, and not restarted in it; turned on before it ends: it stays
        assert!(account_store(&shared, &Who::Owner, turned(true)).await.ok);
        assert_eq!(shared.lock().await.agents["w"].live.account_due.as_deref(), Some("its account is turned off"));
        assert!(shared.lock().await.to_restart().is_empty(), "not in its turn");
        assert!(account_store(&shared, &Who::Owner, turned(false)).await.ok);
        assert!(turn_ends().await.is_empty());
        stayed().await;
        // a removal taken back with Keep it before its turn ends: the same
        turn_begins().await;
        let r = account_remove(&shared, &Who::Owner, "a", false).await;
        assert!(r.ok, "{}", r.out);
        assert_eq!(shared.lock().await.agents["w"].live.account_due.as_deref(), Some("its account is being removed"));
        assert!(account_remove(&shared, &Who::Owner, "a", true).await.ok);
        assert!(turn_ends().await.is_empty());
        stayed().await;
        // still off when its turn ends: it is restarted, and its next start chooses again
        turn_begins().await;
        assert!(account_store(&shared, &Who::Owner, turned(true)).await.ok);
        assert_eq!(turn_ends().await.into_iter().map(|(n, _)| n).collect::<Vec<_>>(), ["w"]);
        assert_eq!(shared.lock().await.agents["w"].live.account_due.as_deref(), Some("its account is turned off"));
    }

    /// The card's `Use` of a kind Claude Code did not apply at this limit is refused before any session starts, as it
    /// is no longer offered (consult-3, N-1437 p7).
    #[tokio::test]
    async fn a_reset_refused_at_this_limit_is_pressed_no_more() {
        let t = Tmp::new("refused-reset");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        let h = Harness::Claude;
        t.store.create(h, "a", "a", 0, Kind::Signin, "x").unwrap();
        let end = accounts::mover::now_s() + 600;
        let refused = accounts::resets::AT_A_LIMIT.map(|k| (k.to_string(), end)).to_vec();
        t.store.update(h, "a", |s| (s.signed_in, s.out_until, s.out_for, s.resets_refused) = (true, Some(end), Some(accounts::limits::RATE_LIMIT.into()), refused)).unwrap();
        let shared = Shared::new(core);
        for kind in accounts::resets::AT_A_LIMIT {
            assert_eq!(account_reset(&shared, &Who::Owner, "a", kind).await, err(format!("coord account reset: Claude Code offers account a no {kind} now")));
        }
        assert!(t.store.state(h, "a").unwrap().reset.is_none(), "no press was claimed");
    }

    /// Q-1441's guard (the owner's yes, 20 Nov): the look of his `Check limits` sends Claude Code `/limit-reset` and
    /// Escape and nothing else, never Enter and never text, on a list of its choices and on the confirm of the one kind
    /// held, where a press would answer Enter; on an answer it sends nothing more, and ends at once (2.1.329's "saved
    /// for a limit" line among them: consult-3, N-1437 post 36). A made-up terminal holder: its screen is the input box
    /// until the command, then the made-up `/limit-reset` screen, then the line Escape leaves.
    #[tokio::test]
    async fn the_look_sends_only_limit_reset_and_escape_and_reads_the_list() {
        use accounts::resets::Look;
        use coord_types::pty::{self, COMMAND, KEYS, TEXT};
        let t = Tmp::new("look");
        let coord = t.dir.join("research").display().to_string();
        std::fs::create_dir_all(pty::dir(&coord)).unwrap();
        let socket = pty::socket(&coord, "reset-a");
        let chooser = "> /limit-reset\n╭──\n│ Which reset do you want to use?\n│ ❯ Use a Fable reset (2 left, until Nov 27)\n│   Use a full reset (1 left, until Dec 3)\n";
        let confirm = "> /limit-reset\n╭──\n│ Use a Fable reset?\n│ Refills your weekly limit now · your weekly reset day stays Monday\n│ 2 Fable resets left · use by Nov 27, 3pm\n│ ❯ Yes, use my Fable reset\n│   No, keep it\n";
        let answer = "> /limit-reset\n  ⎿  No reset to use · you're not at a usage limit right now\n";
        let saved = "> /limit-reset\n  ⎿  You have a reset saved for when you reach a usage limit · run /limit-reset when you do · 2 Fable resets left · use by Nov 27\n";
        let kept = "  ⎿  Resets kept · run /limit-reset when you want one\n> \n";
        for (shows, held) in [(chooser, 2), (confirm, 1), (answer, 0), (saved, 0)] {
            let _ = std::fs::remove_file(&socket);
            let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
            let sent = Arc::new(std::sync::Mutex::new(Vec::<(u8, String)>::new()));
            let got = sent.clone();
            std::thread::spawn(move || {
                for mut c in listener.incoming().flatten() {
                    let Ok((kind, p)) = pty::read(&mut c) else { continue };
                    let mut sent = got.lock().unwrap();
                    if kind == TEXT {
                        let screen = match sent.last().map(|(k, _)| *k) {
                            None => "> \n",
                            Some(COMMAND) => shows,
                            Some(_) => kept,
                        };
                        let _ = std::io::Write::write_all(&mut c, &pty::frame(TEXT, screen.as_bytes()));
                    } else {
                        sent.push((kind, String::from_utf8_lossy(&p).into_owned()));
                    }
                }
            });
            let began = std::time::Instant::now();
            let looked = reset_look(&coord, "reset-a", &socket, accounts::mover::now_s()).await;
            assert!(began.elapsed() < std::time::Duration::from_secs(5), "ended at its first read, not at the session's end: {shows}");
            // the holder takes one connection at a time, in order: once this read is answered, all sent before it is in
            let (c, after) = (coord.clone(), if held == 0 { shows } else { kept });
            assert_eq!(tokio::task::spawn_blocking(move || screen_text(&c, "reset-a")).await.unwrap().as_deref(), Some(after));
            let sent = sent.lock().unwrap().clone();
            match looked {
                Ok(Look::Said(words)) if held == 0 => {
                    assert!(shows.contains(&words), "its words: {words}");
                    assert_eq!(sent, [(COMMAND, "/limit-reset".to_string())], "nothing goes after its answer");
                }
                Ok(Look::Listed(offers)) => {
                    assert_eq!((offers.len(), offers[0].kind.as_str()), (held, "Fable reset"), "{shows}");
                    assert_eq!(sent, [(COMMAND, "/limit-reset".to_string()), (KEYS, "\x1b".to_string())], "only the command and Escape, on {shows}");
                }
                other => panic!("{shows}: {other:?}"),
            }
            assert!(!sent.iter().any(|(_, p)| p.contains('\r') || p.contains('\n')), "never Enter");
        }
    }

    /// The window's Cancel beside "Setting the key…" may come while coordd still checks and starts opencode for the
    /// key, before its run is in `opencode_runs`: it is kept for the key (`opencode_keys`), which ends the run as it
    /// starts (`Run::give_key` then types nothing), and the answer says it is ended.
    #[tokio::test]
    async fn a_cancel_before_a_keys_run_has_started_is_kept_for_it() {
        let t = Tmp::new("oc-key-cancel");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        let shared = Shared::new(core);
        let cancel = || opencode_signin(&shared, &Who::Owner, &t.store, "go", Request::AccountCancel { id: "go".into() });
        assert_eq!(cancel().await, ok("account go: no sign-in runs".into()));
        // a key between `account_key_store`'s take and its run's start
        shared.lock().await.opencode_keys.insert("go".into());
        assert_eq!(cancel().await, ok("account go: its sign-in is ended".into()));
        assert!(!shared.lock().await.opencode_keys.contains("go"), "the key finds it taken out, and ends its run");
        assert_eq!(cancel().await, ok("account go: no sign-in runs".into()), "kept once, for that key alone");
    }

    /// FUT polled once, with the runtime's waker.
    async fn poll_once<F: Future>(mut fut: std::pin::Pin<&mut F>) -> std::task::Poll<F::Output> {
        std::future::poll_fn(|cx| std::task::Poll::Ready(fut.as_mut().poll(cx))).await
    }

    /// consult-6 (N-412 post 2007): the Cancel is served right after the key's first take of the core's lock, before
    /// anything else of the key's. The key's account is in `opencode_keys` from that take, so the Cancel finds it and
    /// says the sign-in is ended. Through the two requests' own functions, nothing put in by hand. The test holds the
    /// lock while both ask for it, the key first; it lets the lock go, gives the key that one take, then serves the
    /// Cancel. On aee6fe34 the key asked a second take to put the account in, and the Cancel waited behind it. The key
    /// is not polled again, so it never reaches opencode's start (no opencode runs here, whatever COORD names): what
    /// that start does after a Cancel is `Run::give_key`'s test.
    #[tokio::test]
    async fn a_cancel_served_right_after_the_keys_first_take_of_the_lock_finds_it() {
        let t = Tmp::new("oc-key-gap");
        let coord = t.dir.join("research");
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = Core::load(&coord.display().to_string(), &format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")));
        core.accounts = t.store.clone();
        t.store.create_opencode("go", "Go", 0, "opencode-go", "x").unwrap();
        let shared = Shared::new(core);
        let held = shared.lock().await;
        let mut key = std::pin::pin!(account_key_store(&shared, Some(Who::Owner), "go".into(), coord_types::Secret::new("sk-made-up-go-0904".into())));
        let mut cancel = std::pin::pin!(account_signin(&shared, &Who::Owner, Request::AccountCancel { id: "go".into() }));
        assert!(poll_once(key.as_mut()).await.is_pending() && poll_once(cancel.as_mut()).await.is_pending(), "both wait on the lock");
        drop(held);
        assert!(poll_once(key.as_mut()).await.is_pending(), "the key's first take; it goes on to check opencode");
        let answered = tokio::time::timeout(std::time::Duration::from_secs(10), cancel).await;
        assert_eq!(answered.expect("the Cancel waits behind a second take of the key's"), ok("account go: its sign-in is ended".into()));
        assert!(shared.lock().await.opencode_keys.is_empty(), "taken out for the key, which ends its run as it starts");
        assert!(holding(&t.dir, "sk-made-up-go-0904").is_empty());
    }
}

#[cfg(test)]
mod owner_roster_tests {
    use super::*;

    fn add(name: &str, dir: Option<&str>) -> Request {
        Request::AddAgent { name: name.into(), model: "claude-haiku-5-5".into(), text: "Read the plan.".into(), dir: dir.map(String::from) }
    }

    #[test]
    fn each_request_is_the_roster_line_the_app_ran() {
        let r = OwnerRoster::of(&add("scout", None)).unwrap();
        assert_eq!(r.args, ["add", "scout", "claude-haiku-5-5", "prompts/scout.md", ""]);
        assert_eq!(r.prompt, Some(("scout".into(), "Read the plan.".into())));
        let f = |new: Option<&str>, model: Option<&str>, text: Option<&str>| OwnerRoster::of(&Request::ForkAgent { from: "lead".into(), new: new.map(String::from), model: model.map(String::from), dir: None, text: text.map(String::from), at: None }).unwrap().args;
        assert_eq!(f(None, None, None), ["fork", "lead"]);
        assert_eq!(f(Some("lead2"), Some("claude-opus-5-5"), Some("Take ask 4.")), ["fork", "lead", "--name", "lead2", "--model", "claude-opus-5-5", "Take ask 4."]);
        // what the app leaves empty is roster's default, not an empty argument
        assert_eq!(f(Some(""), None, Some("  ")), ["fork", "lead"]);
        assert_eq!(OwnerRoster::of(&Request::RenameAgent { name: "a".into(), to: "b".into() }).unwrap().args, ["rename", "a", "b"]);
        assert_eq!(OwnerRoster::of(&Request::State), None);
    }

    #[test]
    fn a_name_that_is_not_one_is_refused() {
        for bad in ["", "../x", "a/b", "-x", ".hidden", "two words"] {
            assert!(OwnerRoster::of(&add(bad, None)).unwrap().bad_name().is_some(), "{bad:?} taken for a name");
            let r = OwnerRoster::of(&Request::RenameAgent { name: "fine".into(), to: bad.into() }).unwrap();
            assert!(r.bad_name().is_some(), "rename to {bad:?} taken");
            let r = OwnerRoster::of(&Request::ForkAgent { from: "lead".into(), new: Some(bad.into()), model: None, dir: None, text: None, at: None }).unwrap();
            assert!(bad.is_empty() || r.bad_name().is_some(), "fork as {bad:?} taken");
        }
        assert_eq!(OwnerRoster::of(&add("scout-2", None)).unwrap().bad_name(), None);
    }

    /// A coord whose agent-base's roster only says how it was run: its arguments, its folder, and the prompt's text.
    fn coord_with_roster(name: &str, exit: u8) -> (std::path::PathBuf, Shared) {
        let dir = std::env::temp_dir().join(format!("owner-roster-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (coord, base) = (dir.join("coord"), dir.join("base"));
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(base.join("bin")).unwrap();
        let script = format!("#!/bin/sh\nprintf '%s|' \"$@\" >\"{}/ran\"; pwd >>\"{}/ran\"; [ \"$1\" != add ] || cat \"$4\" >>\"{}/ran\"\necho \"some line\"\necho \"$2: done\"\nexit {exit}\n", dir.display(), dir.display(), dir.display());
        crate::accounts::testing::program(&base.join("bin/roster"), &script);
        let core = Core::load(&coord.display().to_string(), &base.display().to_string());
        (dir, Shared::new(core))
    }

    #[tokio::test]
    async fn the_owners_new_agent_runs_roster_add_with_its_prompt_written() {
        let (dir, shared) = coord_with_roster("add", 0);
        let coord = dir.join("coord").display().to_string();
        let r = owner_roster(&shared, &Who::Owner, OwnerRoster::of(&add("scout", None)).unwrap()).await;
        assert!(r.ok, "{}", r.out);
        assert_eq!(r.out, "scout: done", "roster's last line is the answer");
        let ran = std::fs::read_to_string(dir.join("ran")).unwrap();
        assert_eq!(ran, format!("add|scout|claude-haiku-5-5|{coord}/prompts/scout.md|{coord}/agents/scout/home|{coord}\nRead the plan.\n"), "no project folder: its own home");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The owner's New agent (the window names no folder) runs in the folder new-project wrote to the coord's `repo`,
    /// and the snapshot's `places.start` says that folder (p2455); with no such folder, or one inside the coord's (an
    /// old new-project's `work`), each agent starts in its own home, made with `sources/` and `project/`, and the
    /// snapshot says `own_homes` (Stream 4A, N-1979); a folder the request names wins.
    #[tokio::test]
    async fn a_new_projects_first_agent_runs_in_the_folder_new_project_wrote_else_its_own_home() {
        let (dir, shared) = coord_with_roster("first", 0);
        let coord = dir.join("coord").display().to_string();
        let work = dir.join("work").display().to_string();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(format!("{coord}/repo"), format!("{work}\n")).unwrap();
        let ran = |r: Reply| {
            assert!(r.ok, "{}", r.out);
            std::fs::read_to_string(dir.join("ran")).unwrap()
        };
        let r = owner_roster(&shared, &Who::Owner, OwnerRoster::of(&add("scout", None)).unwrap()).await;
        assert_eq!(ran(r), format!("add|scout|claude-haiku-5-5|{coord}/prompts/scout.md|{work}|{coord}\nRead the plan.\n"));
        // the same folder is what the snapshot says New agent starts in, for the project's Home (p2455)
        let start = |core: &Core| match core.snapshot_with(Vec::new()).body {
            coord_types::Body::Snapshot(s) => s.places.map(|p| (p.start, p.own_homes)),
            _ => None,
        };
        assert_eq!(start(&*shared.lock().await), Some((Some(work.clone()), false)));
        let r = owner_roster(&shared, &Who::Owner, OwnerRoster::of(&add("scout", Some("/tmp"))).unwrap()).await;
        assert!(ran(r).contains("/scout.md|/tmp|"), "the window's folder wins");
        let home = format!("{coord}/agents/scout/home");
        for gone in [format!("{coord}/gone"), format!("{coord}/work")] {
            std::fs::create_dir_all(format!("{coord}/work")).unwrap();
            std::fs::write(format!("{coord}/repo"), format!("{gone}\n")).unwrap();
            let r = owner_roster(&shared, &Who::Owner, OwnerRoster::of(&add("scout", None)).unwrap()).await;
            assert!(ran(r).contains(&format!("/scout.md|{home}|")), "{gone}: its own home, never the coord's folder");
            assert_eq!(start(&*shared.lock().await), Some((None, true)));
        }
        for part in ["sources", "project/repos", "project/agents"] {
            assert!(std::path::Path::new(&home).join(part).is_dir(), "{part}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// His Add a repository on a project made with no file (fix 3; p2452, p2455): an agent's is refused, nothing
    /// written; the owner's makes the project a file of its own, and from then the snapshot lists the repository,
    /// `places.start` is it, and New agent starts there with its first words still in the coord's `prompts/` (nothing
    /// lies beside the new file for roster to take as the project's settings or standing).
    #[tokio::test]
    async fn his_repository_added_to_a_project_with_no_file_is_where_new_agents_start() {
        let (dir, shared) = coord_with_roster("addrepo", 0);
        let coord = dir.join("coord").display().to_string();
        std::fs::create_dir_all(dir.join("tovrin")).unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q"]).arg(dir.join("tovrin")).status().unwrap().success());
        let repo = std::fs::canonicalize(dir.join("tovrin")).unwrap().display().to_string();
        let asked = || Request::AddRepo { name: "tovrin".into(), path: repo.clone() };
        let r = handle(&shared, &Caller::Agent("lead".into()), 0, asked()).await;
        assert!(!r.ok, "{}", r.out);
        assert!(!std::path::Path::new(&format!("{coord}/project")).exists(), "an agent's: nothing written");
        let r = handle(&shared, &Caller::Owner, 0, asked()).await;
        assert!(r.ok && r.out.contains("definition/project.toml"), "{}", r.out);
        let places = match shared.lock().await.snapshot_with(Vec::new()).body {
            coord_types::Body::Snapshot(s) => s.places.unwrap(),
            _ => unreachable!(),
        };
        assert_eq!((places.repo.as_deref(), places.start.as_deref()), (Some(repo.as_str()), Some(repo.as_str())));
        assert_eq!(places.definition, Some(format!("{coord}/definition")));
        let r = owner_roster(&shared, &Who::Owner, OwnerRoster::of(&add("scout", None)).unwrap()).await;
        assert!(r.ok, "{}", r.out);
        let ran = std::fs::read_to_string(dir.join("ran")).unwrap();
        assert_eq!(ran, format!("add|scout|claude-haiku-5-5|{coord}/prompts/scout.md|{repo}|{coord}\nRead the plan.\n"));
        let beside: Vec<String> = std::fs::read_dir(format!("{coord}/definition")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(beside, ["project.toml"]);
        let r = handle(&shared, &Caller::Owner, 0, asked()).await;
        assert!(!r.ok && r.out.contains("named tovrin already"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Harnesses page's switches are the owner's act from his window: an agent (its cgroup or box) is refused before
    /// the proxy is asked or any file is touched (N-802.p23).
    #[tokio::test]
    async fn an_agent_setting_a_harness_rule_is_refused() {
        let (dir, shared) = coord_with_roster("rules", 0);
        let set = Request::HarnessSet { harness: "claude-code".into(), pairs: vec![coord_types::RulePair { model: "m".into(), provider: "p".into() }], on: false, all_on: false };
        let r = server_read(&shared, &Who::Agent("lead".into()), set.clone()).await;
        assert!(!r.ok && r.out.contains("the owner's"), "{}", r.out);
        let all = Request::HarnessSet { harness: "claude-code".into(), pairs: vec![], on: false, all_on: true };
        assert!(!server_read(&shared, &Who::Agent("lead".into()), all).await.ok);
        // the owner's goes on to the proxy, which this coord has no route to: said, nothing written
        let r = server_read(&shared, &Who::Owner, set).await;
        assert!(!r.ok && r.out.contains("route"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Machines page's numbers are the owner's: an agent is refused before the file is looked for or a sample
    /// starts. The owner's answer is machine-state's tests'; here it would read the build machine's own home.
    #[tokio::test]
    async fn an_agent_asking_for_the_machines_is_refused() {
        let (dir, shared) = coord_with_roster("machines", 0);
        let r = server_read(&shared, &Who::Agent("lead".into()), Request::Machines).await;
        assert!(!r.ok && r.out.contains("the owner's"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_agent_asking_for_the_owners_files_is_refused() {
        let (dir, shared) = coord_with_roster("files", 0);
        let lead = Who::Agent("lead".into());
        for req in [
            Request::Roots,
            Request::ListDir { path: "/".into(), git: false },
            Request::ReadFile { path: "/etc/hostname".into(), from: None, len: None },
            Request::Locate { path: "x".into(), cwd: None, search: vec![] },
            Request::Sources,
            Request::EditFile { path: dir.join("x.kind").display().to_string(), old: None, text: "x".into(), message: "m".into() },
        ] {
            let r = owner_files(&shared, &lead, &req).await.expect("a file request");
            assert!(!r.ok && r.out.contains("the owner's"), "{req:?}: {}", r.out);
        }
        assert!(!dir.join("x.kind").exists());
        assert!(owner_files(&shared, &Who::Owner, &Request::State).await.is_none(), "not a file request");
        assert!(owner_files(&shared, &Who::Owner, &Request::Roots).await.is_some_and(|r| r.ok));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// His rename and his edit of a system prompt (docket-core N-1446 p2471): a rename while his edit waits for its
    /// restart is refused before roster runs, as the restart would find the edit's file moved with the name; while
    /// roster's rename runs no edit is taken (`Core::renaming`, tests/delivery.rs), and once it has answered they are.
    #[tokio::test]
    async fn a_rename_is_refused_while_his_system_prompt_edit_waits() {
        let (dir, _) = coord_with_roster("rename-edit", 0);
        let coord = dir.join("coord");
        std::fs::write(coord.join("registry.jsonl"), format!("{}\n", json!({"ev": "add", "id": "AGT-001", "name": "a", "parent": "owner", "kind": "claude", "model": "m", "at": "x"}))).unwrap();
        let shared = Shared::new(Core::load(&coord.display().to_string(), &dir.join("base").display().to_string()));
        let rename = || OwnerRoster::of(&Request::RenameAgent { name: "a".into(), to: "b".into() }).unwrap();
        shared.lock().await.agents.get_mut("a").unwrap().live.system_due = Some(crate::core::SystemDue { version: 1, replace: false, cut: None });
        let r = owner_roster(&shared, &Who::Owner, rename()).await;
        assert!(!r.ok && r.out.contains("restarts with your edit of its system prompt first"), "{}", r.out);
        assert!(!dir.join("ran").exists(), "roster ran");
        assert!(shared.lock().await.renaming.is_empty());
        shared.lock().await.agents.get_mut("a").unwrap().live.system_due = None;
        let r = owner_roster(&shared, &Who::Owner, rename()).await;
        assert!(r.ok && std::fs::read_to_string(dir.join("ran")).unwrap().starts_with("rename|a|b|"), "{}", r.out);
        assert!(shared.lock().await.renaming.is_empty(), "edits are taken again once roster has answered");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A coord with agents `lead` and its spin-off `lead-fork` (`add` with `from`), and the roster stand-in of
    /// [`coord_with_roster`], whose `ask` prints what the spin-off found.
    fn coord_with_spin(name: &str) -> (std::path::PathBuf, Shared) {
        let (dir, _) = coord_with_roster(name, 0);
        let coord = dir.join("coord");
        let reg = [json!({"ev": "add", "id": "AGT-001", "name": "lead", "parent": "owner", "kind": "claude", "model": "m", "at": "x"}), json!({"ev": "add", "id": "AGT-002", "name": "lead-fork", "parent": "owner", "kind": "claude", "model": "m", "at": "x", "from": "lead"}), json!({"ev": "add", "id": "AGT-003", "name": "other", "parent": "owner", "kind": "claude", "model": "m", "at": "x"})];
        std::fs::write(coord.join("registry.jsonl"), reg.iter().map(|r| format!("{r}\n")).collect::<String>()).unwrap();
        let script = format!("#!/bin/sh\nprintf '%s|' \"$@\" >>\"{}/ran\"; echo >>\"{}/ran\"\n[ \"$1\" != ask ] || {{ echo 'the fix is in parser.rs:40, untested'; exit 0; }}\necho \"$2: done\"\n", dir.display(), dir.display());
        crate::accounts::testing::program(&dir.join("base/bin/roster"), &script);
        // agent-base's own kinds, calls and policy beside the stand-in, so a message is the thread it is
        let real = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        for d in ["kinds", "calls", "policy", "renderings", "views", "lib", "claude"] {
            let _ = std::os::unix::fs::symlink(format!("{real}/{d}"), dir.join("base").join(d));
        }
        let mut core = Core::load(&coord.display().to_string(), &dir.join("base").display().to_string());
        core.load_tickets();
        (dir, Shared::new(core))
    }

    /// His walk item 19 (N-1794): the registry says what a fork was copied from (the window's Spin off), and he ends a
    /// spin-off three ways. Join back asks a copy of its session what it found (`roster ask`) and hands that to the
    /// agent it came from as its own message, then ends it for good; Delete ends it for good with nothing handed back;
    /// Close ends only its session (`roster down`, while it runs) and leaves it there. Refused: an agent's, an agent that
    /// is no spin-off, one already ended, and a Join back while it is in a turn.
    #[tokio::test]
    async fn a_spin_off_is_joined_back_closed_or_deleted() {
        use coord_types::SpinEnd;
        let (dir, shared) = coord_with_spin("spin");
        let ran = || std::fs::read_to_string(dir.join("ran")).unwrap_or_default();
        assert_eq!(shared.lock().await.view("lead-fork").from.as_deref(), Some("lead"), "the registry's add says what it is a copy of");
        assert_eq!(shared.lock().await.view("lead").from, None);
        // refused, nothing run
        let r = spin_end(&shared, &Who::Agent("lead".into()), "lead-fork".into(), SpinEnd::Delete).await;
        assert!(!r.ok && r.out.contains("the owner's"), "{}", r.out);
        let r = spin_end(&shared, &Who::Owner, "other".into(), SpinEnd::Delete).await;
        assert!(!r.ok && r.out.contains("no spin-off"), "{}", r.out);
        {
            let mut core = shared.lock().await;
            let a = core.agents.get_mut("lead-fork").unwrap();
            a.live.state = "working".into();
            a.live.turn = 3;
        }
        let r = spin_end(&shared, &Who::Owner, "lead-fork".into(), SpinEnd::Join).await;
        assert!(!r.ok && r.out.contains("in a turn"), "{}", r.out);
        assert_eq!(ran(), "", "roster ran");
        shared.lock().await.agents.get_mut("lead-fork").unwrap().live.state = "idle".into();
        // Join back: asked, handed to lead in the spin-off's own words, ended
        let r = spin_end(&shared, &Who::Owner, "lead-fork".into(), SpinEnd::Join).await;
        assert!(r.ok && r.out == "lead-fork: joined back into lead", "{}", r.out);
        assert!(ran().starts_with("ask|lead-fork|You were split off from another agent"), "{}", ran());
        {
            let core = shared.lock().await;
            assert!(core.agents["lead-fork"].ended.is_some(), "joined back: in none of his lists");
            let handed = core.tickets.m.tickets.values().flat_map(|t| &t.posts).find(|p| p.by == "lead-fork").expect("its message to lead");
            assert_eq!(handed.names, ["lead"]);
            let words = core.tickets.m.text(&handed.text).map(|w| crate::render::resolve(&w)).unwrap_or_default();
            assert!(words.starts_with("Joined back: lead-fork") && words.contains("the fix is in parser.rs:40, untested"), "{words}");
        }
        let r = spin_end(&shared, &Who::Owner, "lead-fork".into(), SpinEnd::Delete).await;
        assert!(!r.ok && r.out.contains("was ended at"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&dir);

        // Close while it runs: its session ended by roster down, it stays; then Delete: ended, nothing asked
        let (dir, shared) = coord_with_spin("spin-close");
        let ran = || std::fs::read_to_string(dir.join("ran")).unwrap_or_default();
        shared.lock().await.agents.get_mut("lead-fork").unwrap().live = crate::core::Live { pid: Some(1), state: "idle".into(), ..Default::default() };
        let (s2, log) = (shared.clone(), dir.join("ran"));
        // the stand-in's down ends no process: coordd's watch of it is played here
        let watch = tokio::spawn(async move {
            while !std::fs::read_to_string(&log).unwrap_or_default().contains("down|") {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            s2.lock().await.agents.get_mut("lead-fork").unwrap().live.pid = None;
        });
        let r = spin_end(&shared, &Who::Owner, "lead-fork".into(), SpinEnd::Close).await;
        watch.await.unwrap();
        assert!(r.ok && r.out == "lead-fork: closed", "{}", r.out);
        assert_eq!(ran(), "down|lead-fork|\n");
        assert!(shared.lock().await.agents["lead-fork"].ended.is_none(), "closed: it stays");
        let r = spin_end(&shared, &Who::Owner, "lead-fork".into(), SpinEnd::Delete).await;
        assert!(r.ok && r.out == "lead-fork: deleted", "{}", r.out);
        assert_eq!(ran(), "down|lead-fork|\n", "a stopped one: nothing more run, nothing asked");
        assert!(shared.lock().await.agents["lead-fork"].ended.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// His fork of a busy agent (N-1794: "support forking a busy agent cleanly"): roster is given the cut where every
    /// call so far has its result (`fork::settled_end`), so the copy resumes before the call that runs; an idle one's
    /// fork is roster's whole copy, as before.
    #[tokio::test]
    async fn a_busy_agents_fork_is_cut_where_its_calls_are_answered() {
        let (dir, _) = coord_with_roster("fork-busy", 0);
        let coord = dir.join("coord");
        std::fs::write(coord.join("registry.jsonl"), format!("{}\n", json!({"ev": "add", "id": "AGT-001", "name": "lead", "parent": "owner", "kind": "claude", "model": "m", "at": "x"}))).unwrap();
        let shared = Shared::new(Core::load(&coord.display().to_string(), &dir.join("base").display().to_string()));
        let file = dir.join("s1.jsonl").display().to_string();
        let settled = "{\"type\":\"user\",\"message\":{\"content\":\"go\"}}\n";
        let call = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Bash\",\"input\":{}}]}}\n";
        shared.lock().await.merge_session("lead", &json!({"session_id": "s1", "transcript_path": file, "cwd": dir.display().to_string()}));
        let fork = || OwnerRoster::of(&Request::ForkAgent { from: "lead".into(), new: None, model: None, dir: None, text: None, at: None }).unwrap();
        std::fs::write(&file, format!("{settled}{call}")).unwrap();
        let r = owner_roster(&shared, &Who::Owner, fork()).await;
        assert!(r.ok, "{}", r.out);
        assert!(std::fs::read_to_string(dir.join("ran")).unwrap().starts_with(&format!("fork|lead|--cut|{file}|{}|", settled.len())), "in a call: cut before it");
        std::fs::write(&file, settled).unwrap();
        let r = owner_roster(&shared, &Who::Owner, fork()).await;
        assert!(r.ok && std::fs::read_to_string(dir.join("ran")).unwrap().starts_with("fork|lead|/"), "settled: whole, no cut");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_agent_is_refused_and_a_failure_says_roster_s_words() {
        let (dir, shared) = coord_with_roster("refused", 3);
        let r = owner_roster(&shared, &Who::Agent("lead".into()), OwnerRoster::of(&add("scout", None)).unwrap()).await;
        assert!(!r.ok && r.out.contains("the owner's"), "{}", r.out);
        assert!(!dir.join("ran").exists(), "roster ran for an agent");
        let r = owner_roster(&shared, &Who::Owner, OwnerRoster::of(&Request::RenameAgent { name: "a".into(), to: "b".into() }).unwrap()).await;
        assert!(!r.ok && r.out.starts_with("roster rename failed") && r.out.ends_with("a: done"), "{}", r.out);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Run a project source (`project::run_source`) for as long as coordd runs: one that ends is started again, after a
/// pause doubling from 5 s to 5 min while it keeps failing at once. A view whose script died (KeyError after a
/// repository's rename, 8 Nov) stayed dead until coordd's next restart; its last error stays shown meanwhile.
/// Run shared language server L for as long as coordd runs: started again when it ends, as a source is, its ends in
/// the journal.
fn keep_server(dir: String, coord: String, l: project::Lsp) {
    tokio::spawn(async move {
        let mut pause = std::time::Duration::from_secs(5);
        loop {
            let began = std::time::Instant::now();
            let ended = project::run_server(dir.clone(), coord.clone(), l.clone()).await;
            pause = if began.elapsed() > std::time::Duration::from_secs(60) { std::time::Duration::from_secs(5) } else { (pause * 2).min(std::time::Duration::from_secs(300)) };
            eprintln!("coordd: {ended}; started again in {} s", pause.as_secs());
            tokio::time::sleep(pause).await;
        }
    });
}

fn keep_source(dir: String, cmd: String, send: impl Fn(Option<Value>, Option<String>) + Clone + Send + Sync + 'static) {
    tokio::spawn(async move {
        let mut pause = std::time::Duration::from_secs(5);
        loop {
            let began = std::time::Instant::now();
            project::run_source(dir.clone(), cmd.clone(), send.clone()).await;
            pause = if began.elapsed() > std::time::Duration::from_secs(60) { std::time::Duration::from_secs(5) } else { (pause * 2).min(std::time::Duration::from_secs(300)) };
            tokio::time::sleep(pause).await;
        }
    });
}

/// A line of the project's event source (`Core::project_event`). What agents wait on follows from the tickets it
/// answers (a check's `check_end`), not from a waits source (plan/edge.md S4).
async fn project_event(shared: Shared, data: Option<Value>, error: Option<String>) {
    let Some(v) = data else { return eprintln!("coordd: project events: {}", error.unwrap_or_default()) };
    shared.lock().await.project_event(v.clone()).await;
    // the runner's running jobs read on each of its events while one runs (runner.rs; N-101 post 27, D)
    runner::after_event(&shared, &v).await;
}

/// Count each agent's tokens from its sessions' transcripts (transcript.rs), reading on every few seconds, off the lock
/// (the first round reads every transcript whole), and keep the requests read (`Core::add_sent`). An agent's view changes when its counts do, at most once a round, so
/// a busy session does not flood the stream.
/// The models the places serve and their families for the norms (`Core::set_models`), read off the lock once a minute
/// (`Router::served`; a failed read of the gateway gives the list it gave last); the agents whose model the lists the
/// gateway gave have not named for ten minutes, the first read ten minutes ago or more, go onto the owner's accounts
/// (`Core::gateway_lost`); and then, off the
/// lock too, the model of each task the watcher's steps run (`Core::set_task_models`), solved again only when the
/// policy's files, those tasks or the models served changed (consult-3, N-1396 p237).
fn read_models(shared: Shared) {
    tokio::spawn(async move {
        loop {
            let places = shared.lock().await.places();
            let read = tokio::task::spawn_blocking(move || accounts::router::Router::of(&places).served()).await;
            if let Ok(served) = read {
                let gateway = served.read.clone().ok().flatten();
                let families: Vec<(String, String)> = served.models().unwrap_or_default().into_iter().map(|(id, p)| (id, policy::facts::family(&p).to_string())).collect();
                let ask = {
                    let mut core = shared.lock().await;
                    if let Some(list) = &gateway {
                        core.gateway_lost(list);
                    }
                    // none served (nothing read, nothing kept, no account): the models and tasks stay as they were
                    (!families.is_empty()).then(|| {
                        core.set_models(families.clone());
                        core.task_models_ask()
                    })
                };
                if let Some(ask) = ask {
                    if let Ok(Some(solved)) = tokio::task::spawn_blocking(move || policy::solve_task_models(ask, &families)).await {
                        shared.lock().await.set_task_models(solved);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
    });
}

/// Each agent's jobs on the build runner (N-1334): the runner's job files and kpi-data's file of the machines read
/// off the lock every second (`jobs::read`, `jobs::machine`), then given to `Core::set_jobs`, which sends a view only
/// when an agent's jobs changed.
fn read_jobs(shared: Shared) {
    tokio::spawn(async move {
        loop {
            if let Ok((read, machine)) = tokio::task::spawn_blocking(|| (jobs::read(&jobs::dir()), jobs::machine(&jobs::machines_file(), std::time::SystemTime::now()))).await {
                shared.lock().await.set_jobs(read, machine);
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
}

/// The agents' side of the kept graph (`follow::KeptSide`, paper-research N-1446 p1458): each 5 s the sessions the
/// record's `given` lines name are read on from where they left off, off the lock, and put in under it, a hold for each
/// session ([`put_side`]). On the same clock git's side (p1476): the folders where agents work placed, each
/// repository whose refs moved read (its commits and its reflogs), the first time with the runner's jobs from before
/// the record had them (p1422, 2), all off the lock, and put in under it ([`core::Core::graph_git_put`]).
fn read_sides(shared: Shared) {
    tokio::spawn(counts::scoped(async move {
        let home = std::env::var("HOME").unwrap_or_default();
        loop {
            let side_round = counts::Round::new("clock side (each 5 s)");
            let round = shared.lock().await.side_round();
            let Ok(round) = tokio::task::spawn_blocking(move || round.read()).await else { return };
            put_side(&shared, round, follow::SIDE_PART).await;
            drop(side_round);
            let git_round = counts::Round::new("clock git (each 5 s)");
            let (reading, coord) = {
                let c = shared.lock().await;
                (c.graph_git_reading(), (!c.tickets.graph_jobs_read()).then(|| c.coord.clone()))
            };
            let h = home.clone();
            let read = tokio::task::spawn_blocking(move || {
                let before = coord.map(|coord| jobs::of_coord(&jobs::before_record_dir(), &coord));
                let read = reading.read(before.as_deref().unwrap_or_default(), &h);
                (before, read)
            })
            .await;
            // a read that failed puts nothing and stops neither clock: the next round reads again
            if let Ok((before, read)) = read {
                shared.lock().await.graph_git_put(read, before);
            }
            drop(git_round);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }));
}

/// ROUND's sessions put in the kept graph, each in parts of about MOST edges, each part under its own hold of the
/// core's lock (`SideRound::each`, `Core::side_put_part`), so what waits for the lock, a delivery or his window's ask,
/// waits for one part's put at most: the first round's whole put held it 2.6 s on research's copy (run 10,
/// paper-research N-1446 p1474), and one session's whole put up to 158 ms (p1919; in parts, p1922 (a)).
async fn put_side(shared: &Shared, round: follow::SideRound, most: usize) {
    for mut one in round.each() {
        while !shared.lock().await.side_put_part(&mut one, most) {}
    }
}

fn count_usage(shared: Shared) {
    tokio::spawn(counts::scoped(async move {
        // what was read before this start, kept in the coord's folder (`transcript::KeptUsage`): only what is new is
        // read; a file it does not fit, or all of them when it is lost, are read from the start
        let kept_path = format!("{}/usage-kept.json", shared.lock().await.coord);
        let kept = {
            let p = kept_path.clone();
            tokio::task::spawn_blocking(move || transcript::KeptUsage::load(&p)).await.unwrap_or_default()
        };
        shared.lock().await.restore_sent(kept.sent);
        let mut files: HashMap<String, TranscriptUsage> = kept.files;
        let mut saved = std::time::Instant::now();
        let mut unsaved = false;
        loop {
            let usage_round = counts::Round::new("clock usage (each 5 s)");
            let (sessions, running, coord, base, accounts, opener) = {
                let core = shared.lock().await;
                (core.sessions(), core.running_names(), core.coord.clone(), core.base.clone(), core.accounts.clone(), core.opener_times())
            };
            // the opener's rule goes into the round, and stays here for the looks it claims
            let (store, times) = (accounts.clone(), opener.clone());
            let round = tokio::task::spawn_blocking(move || {
                let cgroups: Vec<(String, (Option<u64>, Option<f64>))> = running.into_iter().map(|n| { let u = cgroup::usage(&n); (n, u) }).collect();
                let window = core::Core::context_window(&coord, &base);
                let counted: Vec<(String, Vec<ModelUsage>, Option<String>, Vec<Ended>, Option<coord_types::AgentContext>)> = sessions
                    .into_iter()
                    .map(|(name, paths)| {
                        let (mut by, mut last, mut ended) = (ByModel::new(), None, Vec::new());
                        let mut latest: Option<(i64, u64, Option<u64>)> = None;
                        // its context by kind, from the file that holds its latest request
                        let mut parts = Vec::new();
                        for path in paths.iter().flat_map(|p| transcript::session_files(p)) {
                            let u = files.entry(path.clone()).or_default();
                            u.read(&path);
                            ended.append(&mut u.ended);
                            if let Some(l) = u.latest_context().filter(|l| latest.is_none_or(|m| l.0 > m.0)) {
                                latest = Some(l);
                                parts = u.context_parts(l.1);
                            }
                            for (k, (all, long)) in &u.by {
                                let e = by.entry(k.clone()).or_default();
                                e.0 += *all;
                                e.1 += *long;
                            }
                            last = last.max(u.last.clone());
                        }
                        let usage = by
                            .into_iter()
                            .filter(|(_, (tokens, _))| tokens.total() > 0)
                            .map(|((model, tier), (tokens, long))| ModelUsage { model, tier, tokens, long })
                            .collect();
                        // a file that says its model's window is a Codex session's: the coord's point
                        // is Claude Code's setting and says nothing of where Codex compacts
                        let context = latest.map(|(_, used, whole)| coord_types::AgentContext { used, window: if whole.is_some() { None } else { window }, max: whole, parts, ..Default::default() });
                        (name, usage, last, ended, context)
                    })
                    .collect();
                // the accounts: can one take requests, and is one being removed (a few small files); and the window
                // opener's looks that are due, claimed by this coordd (accounts/opener.rs)
                let looks = opener.as_ref().map(|t| accounts::opener::due(&accounts, t, accounts::mover::now_s())).unwrap_or_default();
                (files, counted, cgroups, accounts::mover::seen(&accounts), looks)
            });
            let Ok((f, counted, cgroups, seen, looks)) = round.await else { return };
            if let Some(t) = &times {
                for id in looks {
                    tokio::spawn(window_look(shared.clone(), store.clone(), id, t.clone()));
                }
            }
            files = f;
            let mut core = shared.lock().await;
            // the switch that turns on by itself, who waited for a sign-in, and this coord's agents on an account
            // that is being removed; one that no agent runs on any more is removed, off the lock
            for id in core.accounts_seen(&seen) {
                tokio::spawn(account_left(shared.clone(), store.clone(), id));
            }
            for (name, (mem, cpu)) in cgroups {
                core.set_cgroup(&name, mem, cpu);
            }
            for (name, usage, last, ended, context) in counted {
                unsaved |= !ended.is_empty();
                core.set_usage(&name, usage, last);
                core.set_context(&name, context);
                core.add_sent(&name, ended);
            }
            // kept at most once a minute, when something was read
            if unsaved && saved.elapsed() >= std::time::Duration::from_secs(60) {
                let k = transcript::KeptUsage { files: files.iter().map(|(p, u)| (p.clone(), u.kept())).collect(), sent: core.sent_kept() };
                let p = kept_path.clone();
                tokio::task::spawn_blocking(move || k.save(&p));
                (saved, unsaved) = (std::time::Instant::now(), false);
            }
            drop(core);
            drop(usage_round);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }));
}

/// Each agent's transcript and folder sizes, every `footprint::EVERY`, in a task of its own: a large folder never holds
/// the usage timer, and the walk runs off the lock.
fn count_footprints(shared: Shared) {
    tokio::spawn(counts::scoped(async move {
        let mut kept = footprint::Kept::default();
        loop {
            let (coord, agents) = {
                let core = shared.lock().await;
                (core.coord.clone(), core.footprint_paths())
            };
            let Ok((k, sizes)) = tokio::task::spawn_blocking(move || {
                let sizes = kept.read(std::path::Path::new(&coord), agents);
                (kept, sizes)
            })
            .await
            else {
                return;
            };
            kept = k;
            let mut core = shared.lock().await;
            for (name, f) in sizes {
                core.set_footprint(&name, f);
            }
            drop(core);
            tokio::time::sleep(footprint::EVERY).await;
        }
    }));
}

/// When PID exits, agent NAME stopped.
fn watch_exit(shared: Shared, name: String, pid: i32) {
    tokio::spawn(async move {
        exit_of(pid).await;
        shared.lock().await.exited(&name, pid);
    });
}

/// Once PID has exited, however it died (at once if it is gone or cannot be watched), its pidfd closed again: coordd
/// holds one for each agent alive and none after (paper-research N-1396 p376).
async fn exit_of(pid: i32) {
    if let Ok(fd) = cgroup::pidfd(pid) {
        // SAFETY: the pidfd is ours alone and lives inside the AsyncFd until it is dropped here
        if let Ok(afd) = unsafe { AsyncFd::register(fd) } {
            let _ = afd.readable().await;
        }
    }
}

async fn serve(shared: Shared, stream: UnixStream) {
    let pid = stream.peer_cred().ok().and_then(|c| c.pid()).unwrap_or(0);
    let path = cgroup::path_of(pid);
    let caller = cgroup::identify_path(&path);
    let (read, mut write) = stream.into_split();
    let mut lines = BufReader::new(read).lines();
    // its request is waited for while it is connected: a client that ends closes the socket and the read ends (no time
    // limit: the owner, N-68)
    let Ok(Some(line)) = lines.next_line().await else { return };
    let caller = declared_caller(&shared, caller, &path, &line).await;
    // an account's sign-in program (Claude Code's, opencode's or t3code's, fetched from outside) is answered nothing:
    // its line is read for a declared agent alone (above), and nothing it asks is done (docket-core3, N-946 post 11)
    if caller == Caller::SignIn {
        drop(line);
        return send(&mut write, &err("coordd: a sign-in's program is answered nothing".into())).await;
    }
    let req: Request = match serde_json::from_str(&line) {
        Ok(r) => r,
        Err(e) => return send(&mut write, &err(format!("coordd: not a request: {e}"))).await,
    };
    if let Request::Watch { from, run, box_id } = req {
        // the window this watch is for is open while it is up (lapse.rs Boxes)
        if let Some(b) = &box_id {
            shared.lock().await.boxes.up(b, std::time::Instant::now());
        }
        watch(shared.clone(), pid, from, run, &mut lines, &mut write).await;
        if let Some(b) = &box_id {
            shared.lock().await.boxes.down(b, std::time::Instant::now());
        }
        return;
    }
    if let Request::AccountKeyStore { id, key } = req {
        // the line holds the key: overwritten here, and the request goes by this path alone, which has no record, no
        // journal line of a request, no event and no second reading of the request (`handle` never sees it); the
        // reading of a declared name before it (`declared_caller`) skips every other field, the key with them
        let mut held = line.into_bytes();
        held.fill(0);
        drop(std::hint::black_box(held));
        let reply = counts::timed("request accountkeystore", account_key_store(&shared, who(&caller), id, key)).await;
        return send(&mut write, &reply).await;
    }
    if let (Request::AskWait { text, context }, Caller::Agent(me)) = (&req, &caller) {
        let reply = ask_wait(&shared, &caller, pid, me, text, context.clone(), lines).await;
        return send(&mut write, &reply).await;
    }
    let op = timed_as(&req);
    let reply = counts::timed(&format!("request {op}"), handle(&shared, &caller, pid, req)).await;
    send(&mut write, &reply).await;
}

/// CALLER (from cgroup PATH), or the agy agent its request LINE declares (`coord_types::declared`), when
/// `accounts::agy::declares` takes that name: the one exception to a caller's cgroup naming it (paper-research N-1446
/// p2099). Only a caller that is no agent and no project source by its cgroup is asked about: the owner, by the cgroup
/// of another coordd's t3code server, or a sign-in, by this coordd's own. Every other caller's declaration is
/// ignored, and its cgroup's name stands.
async fn declared_caller(shared: &Shared, caller: Caller, path: &str, line: &str) -> Caller {
    let name = match caller {
        Caller::Owner | Caller::SignIn => coord_types::declared(line),
        Caller::Agent(_) | Caller::Unclaimed | Caller::Project => None,
    };
    let Some(name) = name else { return caller };
    let (store, coord) = {
        let core = shared.lock().await;
        (core.accounts.clone(), core.coord_id())
    };
    if accounts::agy::declares(&store, &coord, &name, path) {
        Caller::Agent(name)
    } else {
        caller
    }
}

/// How long an ask waits in the asker's turn (paper-research m1162, 8); COORDD_ASK_WAIT (seconds) for a test.
fn ask_wait_for() -> std::time::Duration {
    std::time::Duration::from_secs(std::env::var("COORDD_ASK_WAIT").ok().and_then(|s| s.parse().ok()).unwrap_or(600))
}

/// What the ask tool says when the wait is cut: never an error, never a nudge to answer the step another way
/// (paper-research m1171, m1197, m1218).
const ASK_LATER: &str = "No answer yet. The answer will come later, as a message to you; your step stays yours and waits for it. Go on with any work that does not depend on it; end your turn if nothing else is left.";

/// The ask tool's ask by agent ME: it waits on the ticket ME holds a step of (`ticket_ask_wait`) until the answer lands
/// in `answered[ME]` (woken by `answered_tx`), ASK_WAIT passes, or the caller hangs up (the tool call cancelled: an esc); the last two say the
/// answer comes later (`ticket_ask_later`), and it then comes as a delivery. Where an ask cannot wait, it is a question
/// ticket, as `coord ask` makes, and the reply says so at once.
async fn ask_wait(shared: &Shared, caller: &Caller, pid: i32, me: &str, text: &str, context: Option<String>, mut rest: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>) -> Reply {
    let waits = shared.lock().await.ticket_ask_wait(me, text, context.clone());
    if let Err(why) = waits {
        eprintln!("coordd: {me}'s ask does not wait: {why}");
        let body = match &context {
            Some(c) => format!("{text}\n{c}"),
            None => text.to_string(),
        };
        return handle(shared, caller, pid, Request::Ask { body, on: None }).await;
    }
    let deadline = tokio::time::Instant::now() + ask_wait_for();
    // subscribed before each look at answered, so an answer put there after the look still wakes it
    let mut woken = shared.lock().await.tickets.answered_tx.subscribe();
    loop {
        let cut = {
            let mut core = shared.lock().await;
            if let Some(answer) = core.tickets.answered.remove(me) {
                return ok(answer);
            }
            drop(core);
            tokio::select! {
                _ = woken.changed() => false,
                _ = tokio::time::sleep_until(deadline) => true,
                // the caller writes nothing more: a line or the end is the call cancelled
                _ = rest.next_line() => true,
            }
        };
        if cut {
            let mut core = shared.lock().await;
            if let Err(e) = core.ticket_ask_later(me) {
                eprintln!("coordd: {me}'s ask: {e}");
            }
            // an answer that came with the cut is still this ask's
            return ok(core.tickets.answered.remove(me).unwrap_or_else(|| ASK_LATER.to_string()));
        }
    }
}

async fn send(w: &mut tokio::net::unix::OwnedWriteHalf, reply: &Reply) {
    let _ = w.write_all(format!("{}\n", serde_json::to_string(reply).unwrap()).as_bytes()).await;
}

/// The stream: a snapshot (or, resumed in RUN, `Resumed` and the events after FROM, when coordd still has them:
/// `Core::resume`), then each change. It ends when its
/// client goes: its socket is read too (a watcher sends nothing more), so a client gone ends it at once, not at the
/// next event, which on a quiet coord may be hours away (docket-try, 12 Nov: a descriptor held per yaaihh gone).
/// Who process PID is, for the watch lines: its command and its parent's, each cut at 100 characters, every
/// `NAME=VALUE` word without its value (a `docker exec -e` passes variables so, and the journal holds no values).
fn process_words(pid: i32) -> String {
    let cmd = |p: i32| {
        let raw = std::fs::read(format!("/proc/{p}/cmdline")).unwrap_or_default();
        let words: String = String::from_utf8_lossy(&raw).split('\0').filter(|w| !w.is_empty()).map(|w| match w.split_once('=') {
            Some((k, _)) => format!("{k}=…"),
            None => w.to_string(),
        }).collect::<Vec<_>>().join(" ");
        if words.is_empty() { String::from("?") } else { words.chars().take(100).collect() }
    };
    let ppid = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok().and_then(|s| s.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)?.parse::<i32>().ok())).unwrap_or(0);
    format!("pid {pid} `{}`, parent {ppid} `{}`", cmd(pid), cmd(ppid))
}

/// A watch from PID: its first events (a resume from FROM of RUN, else a whole snapshot), then everything that comes.
/// Each watch says in the journal who opened it and whether it resumed or had a whole snapshot, each whole snapshot
/// again for falling behind, and its end, with how long it lasted and how many whole snapshots it had (paper-research
/// N-447 p43: 216 snapshots in bursts at 14:13–14:18, from a client nobody could name).
async fn watch(shared: Shared, pid: i32, from: Option<u64>, run: Option<String>, r: &mut tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>, w: &mut tokio::net::unix::OwnedWriteHalf) {
    let who = process_words(pid);
    let began = std::time::Instant::now();
    let (resumed, mut rx, lock) = {
        let core = shared.lock().await;
        let got = std::time::Instant::now();
        (from.and_then(|f| core.resume(f, run.as_deref())), core.tx.subscribe(), held_and_waited(began, got))
    };
    // a whole snapshot is built a few tickets at a time, the lock let go between (paper-research N-560 p29, 2)
    let (first, resumed, lock) = match resumed {
        Some(events) => (events, true, lock),
        None => {
            let (snap, held) = counts::timed("watch snapshot (a few tickets at a time)", chunked_snapshot(&shared, &mut rx)).await;
            (vec![snap], false, held)
        }
    };
    let asked = match (&from, &run) {
        (Some(f), Some(r)) => format!("asked to resume from {r}:{f}"),
        (Some(f), None) => format!("asked to resume from {f}"),
        _ => String::from("asked for a snapshot"),
    };
    eprintln!("coordd: watch from {who}: {asked}; {} ({lock})", if resumed { "resumed" } else { "a whole snapshot" });
    let mut snapshots = u32::from(!resumed);
    let end = |snapshots: u32, why: &str| eprintln!("coordd: watch from pid {pid} ended ({why}) after {} s, {snapshots} whole snapshot(s)", began.elapsed().as_secs());
    let mut last = 0;
    for ev in first {
        last = ev.seq;
        if write_event(w, &ev).await.is_err() {
            return end(snapshots, "its first events could not be written");
        }
    }
    loop {
        let got = tokio::select! {
            got = rx.recv() => got,
            line = r.next_line() => match line {
                Ok(Some(_)) => continue,
                _ => return end(snapshots, "the client closed it"),
            },
        };
        match got {
            Ok(ev) if ev.seq <= last => {}
            Ok(ev) => {
                last = ev.seq;
                if write_event(w, &ev).await.is_err() {
                    return end(snapshots, "an event could not be written");
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                snapshots += 1;
                let (snap, held) = counts::timed("watch snapshot (a few tickets at a time)", chunked_snapshot(&shared, &mut rx)).await;
                eprintln!("coordd: watch from pid {pid} fell {n} events behind: a whole snapshot again (its {snapshots}; {held})");
                last = snap.seq;
                if write_event(w, &snap).await.is_err() {
                    return end(snapshots, "a snapshot could not be written");
                }
            }
            Err(_) => return end(snapshots, "coordd's stream ended"),
        }
    }
}

/// For a watch's journal line: how long it held coordd's lock, from GOT (when it had it) to now, and how long it
/// waited for it, from ASKED to GOT (paper-research N-518 p23: after a restart the first snapshot came 13 to 19 s late,
/// and the line did not say whether the snapshot or the wait for the lock was the time).
/// The last hold of `chunked_snapshot`: CORE's snapshot with the tickets' VIEWS built before, each ticket an event on
/// RX changed since (or every one, when RX fell behind) built again, and the tickets shown now taken as they are.
fn snapshot_finished(core: &Core, mut views: std::collections::BTreeMap<u64, docket_machine::TicketView>, rx: &mut tokio::sync::broadcast::Receiver<coord_types::Event>) -> coord_types::Event {
    // what changed since the views were begun: their tickets built again
    let (mut changed, mut all) = (std::collections::BTreeSet::new(), false);
    loop {
        match rx.try_recv() {
            Ok(ev) => match &ev.body {
                coord_types::Body::Ticket(v) | coord_types::Body::TicketChange { view: v, .. } => {
                    changed.extend(docket_machine::parse_ticket(&v.id));
                }
                coord_types::Body::Mentioned { id, .. } => {
                    changed.extend(docket_machine::post_or_ticket(id).and_then(|(t, _)| docket_machine::parse_ticket(t)));
                }
                _ => {}
            },
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => all = true,
            Err(_) => break,
        }
    }
    let tickets: Vec<docket_machine::TicketView> = core
        .ticket_view_ids(|_| true)
        .into_iter()
        .filter_map(|t| match views.remove(&t) {
            Some(v) if !all && !changed.contains(&t) => Some(v),
            _ => core.ticket_shown(t),
        })
        .collect();
    counts::timed_sync("watch snapshot", || core.snapshot_with(tickets))
}

/// How many tickets' views a whole snapshot builds in one hold of the lock (`chunked_snapshot`).
const SNAPSHOT_CHUNK: usize = 16;

/// A whole snapshot for a watch subscribed as RX, without holding the lock for all of it (paper-research N-560 p29, 2:
/// research's took 2-4.8 s under it, every request of his window waiting). The tickets' views (with their mentions,
/// whose labels read posts' words from the transcripts) are built [`SNAPSHOT_CHUNK`] at a time with the lock let go
/// between; then, in one last hold, every ticket an event changed meanwhile (the events on RX, all of them, since
/// nothing is sent but under the lock) is built again, the tickets shown now are taken as they are, and the rest of the
/// snapshot is built with the seq of that moment. So it is what one hold would have given then, and the events after it
/// follow it as before. The events read from RX meanwhile have that seq or less: the watch would pass them over.
/// Also says the longest hold and the time waited for the lock, as `held_and_waited`.
async fn chunked_snapshot(shared: &Shared, rx: &mut tokio::sync::broadcast::Receiver<coord_types::Event>) -> (coord_types::Event, String) {
    let began = std::time::Instant::now();
    let (mut longest, mut waited) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    let ids = {
        let core = shared.lock().await;
        core.ticket_view_ids(|_| true)
    };
    let mut views: std::collections::BTreeMap<u64, docket_machine::TicketView> = std::collections::BTreeMap::new();
    for chunk in ids.chunks(SNAPSHOT_CHUNK) {
        let asked = std::time::Instant::now();
        let core = shared.lock().await;
        let got = std::time::Instant::now();
        for &t in chunk {
            if let Some(v) = core.ticket_shown(t) {
                views.insert(t, v);
            }
        }
        drop(core);
        waited += got - asked;
        longest = longest.max(got.elapsed());
    }
    let asked = std::time::Instant::now();
    let core = shared.lock().await;
    let got = std::time::Instant::now();
    waited += got - asked;
    let snap = snapshot_finished(&core, views, rx);
    drop(core);
    longest = longest.max(got.elapsed());
    (snap, format!("held the lock {} ms at most, waited {} ms for it, in {} ms", longest.as_millis(), waited.as_millis(), began.elapsed().as_millis()))
}

fn held_and_waited(asked: std::time::Instant, got: std::time::Instant) -> String {
    format!("held the lock {} ms, waited {} ms for it", got.elapsed().as_millis(), got.duration_since(asked).as_millis())
}

async fn write_event(w: &mut tokio::net::unix::OwnedWriteHalf, ev: &Event) -> std::io::Result<()> {
    let text = counts::timed_sync(&format!("watch write {}", counts::body_kind(&ev.body)), || serde_json::to_string(ev).unwrap());
    w.write_all(format!("{text}\n").as_bytes()).await
}

fn who(caller: &Caller) -> Option<Who> {
    match caller {
        Caller::Owner => Some(Who::Owner),
        Caller::Agent(a) => Some(Who::Agent(a.clone())),
        _ => None,
    }
}

/// The program whose sign-in coordd runs: Claude Code, as roster starts it (found on coordd's PATH), the one the router
/// runs a call on an account with.
use accounts::router::CLAUDE;

/// The sign-in requests of an account (accounts/signin.rs), off the core's lock: `AccountSignin` starts Claude Code's
/// `auth login` under the account's folder and answers its URL; `AccountCode` hands it the code the browser showed;
/// `AccountCancel` ends it; `AccountCheck` asks Claude Code whether the account is signed in and records it. A
/// sign-in's end, however it comes, is recorded by `signin_ended`.
async fn account_signin(shared: &Shared, who: &Who, req: Request) -> Reply {
    use accounts::{signin, Harness};
    let h = Harness::Claude;
    let id = match &req {
        Request::AccountSignin { id, .. } | Request::AccountCode { id, .. } | Request::AccountCheck { id } | Request::AccountCancel { id } => id.clone(),
        _ => return err("coordd: not a sign-in request".into()),
    };
    if !matches!(who, Who::Owner) && !matches!(req, Request::AccountCheck { .. }) {
        return err("coord account: an account's sign-in is the owner's".into());
    }
    let (store, running) = {
        let core = shared.lock().await;
        (core.accounts.clone(), core.signins.get(&id).filter(|s| s.ended().is_none()).cloned())
    };
    if store.harness_of(&id) == Some(Harness::Opencode) {
        return opencode_signin(shared, who, &store, &id, req).await;
    }
    if store.harness_of(&id) == Some(Harness::Agy) {
        return agy_signin(shared, who, &store, &id, req).await;
    }
    let Some(entry) = store.get(h, &id) else { return err(format!("coord account: no account {id} (coord account new {id})")) };
    // a key account has no sign-in: only the check is answered for it, from its key file
    if entry.account.kind == accounts::Kind::Key && !matches!(req, Request::AccountCheck { .. }) {
        return err(format!("coord account: account {id} runs on a key, not on a sign-in (coord account key {id})"));
    }
    if entry.state.leaving.is_some() && matches!(req, Request::AccountSignin { .. }) {
        return err(format!("coord account signin: account {id} is being removed"));
    }
    let config = store.config(h, &id);
    match req {
        Request::AccountSignin { email, .. } => {
            let s = match running {
                Some(s) => s,
                None => {
                    if let Err(why) = store.layout(h, &id) {
                        return err(format!("coord account signin: {why}"));
                    }
                    let s = match signin::SignIn::start(std::path::Path::new(CLAUDE), &config, email.as_deref()) {
                        Ok(s) => Arc::new(s),
                        Err(why) => return err(format!("coord account signin: {why}")),
                    };
                    // in a cgroup of its own, neither coordd's nor an agent's (paper-research, N-412 p148), for as long
                    // as the owner takes in the browser; a sign-in that cannot be put there is not run
                    if let Err(e) = cgroup::place(s.pid(), &signin_group(&id)) {
                        s.cancel().await;
                        return err(format!("coord account signin: its command could not be put in a cgroup of its own ({}): {e}", cgroup::root().join(signin_group(&id)).display()));
                    }
                    // two requests at once may both have found none running: the one that is on file by now is the
                    // account's sign-in, and this one is ended, so that none runs that no request can reach
                    let mut core = shared.lock().await;
                    match core.signins.get(&id).filter(|x| x.ended().is_none()).cloned() {
                        Some(first) => {
                            drop(core);
                            s.cancel().await;
                            first
                        }
                        None => {
                            core.signins.insert(id.clone(), s.clone());
                            tokio::spawn(signin_ended(shared.clone(), id.clone(), s.clone()));
                            s
                        }
                    }
                }
            };
            match s.url().await {
                Ok(url) => ok(format!("{url}\nOpen it and sign in. If the browser then shows a code: coord account code {id} CODE. Then: coord account check {id}")),
                Err(why) => err(format!("coord account signin: {why}")),
            }
        }
        Request::AccountCode { code, .. } => {
            let Some(s) = running else { return err(format!("coord account code: no sign-in runs for {id} (coord account signin {id})")) };
            if let Err(why) = s.paste(&code).await {
                return err(format!("coord account code: {why}"));
            }
            // waited for as long as the sign-in's command lives; one that asks again instead of ending is seen with
            // `coord account check` (its words so far) and ended with `coord account cancel`
            match s.wait().await {
                signin::Ended::Ok => account_check(shared, &id, true).await,
                signin::Ended::Failed(why) => err(format!("coord account code: the sign-in failed: {why}")),
            }
        }
        Request::AccountCancel { .. } => match running {
            Some(s) => {
                s.cancel().await;
                ok(format!("account {id}: its sign-in is ended"))
            }
            None => ok(format!("account {id}: no sign-in runs")),
        },
        _ => {
            let r = account_check(shared, &id, matches!(who, Who::Owner)).await;
            // his `Check limits` also reads what Claude Code's `/limit-reset` lists of a signed-in sign-in account
            // (Q-1441): the press his card shows for the account as it stood before the check, not his `Check
            // whether it is signed in`, and only while it is still signed in after it
            let limits_press = accounts::resets::looks_on_check(&entry.state, accounts::mover::now_s());
            if matches!(who, Who::Owner) && r.ok && entry.account.kind == accounts::Kind::Signin && limits_press && store.state(h, &id).is_some_and(|s| s.signed_in && s.leaving.is_none()) {
                resets_look(shared, &id).await;
            }
            // a running sign-in's words, its URL among them, are the owner's; an agent learns only whether it is
            // signed in
            match running.filter(|_| matches!(who, Who::Owner)) {
                Some(s) if r.ok => ok(format!("{}\na sign-in runs for it; its words so far:\n{}", r.out, s.printed())),
                _ => r,
            }
        }
    }
}

/// `Request::AccountKeyStore`, the key field of the window's Accounts page (paper-research, N-591 post 11), off the
/// core's lock: the owner's API key for key account ID is written to the account's key file (`keyfile::write`, mode
/// 600) and is nowhere else when this returns. KEY is moved into the task that writes it and dropped there, which
/// overwrites its bytes (`coord_types::Secret`); nothing is copied out of it but the file. The answer says stored or
/// not stored and why, and no word of it comes from the key.
///
/// The owner's only: WHO is read from the caller's cgroup as for every request, so an agent, a terminal holder and a
/// project source are refused before anything is done (the key they sent is dropped with the request). After the
/// write it is the terminal's last step (`Core::account_key`, `set`): the file is looked at, never read, the account
/// can take agents, and the journal says `account ID: its key is set`.
async fn account_key_store(shared: &Shared, who: Option<Who>, id: String, key: coord_types::Secret) -> Reply {
    use accounts::{Harness, Kind};
    let not_stored = |why: &str| err(format!("account {id}: key not stored: {why}"));
    let Some(Who::Owner) = who else { return not_stored("a key is given in the owner's window only") };
    // an opencode account's key waits in `opencode_keys` from the take of the lock that finds the account opencode's:
    // a Cancel served after that take finds it (consult-6, N-412 post 2007), however the lock's next takes fall
    let (store, oc) = {
        let mut core = shared.lock().await;
        let oc = core.accounts.harness_of(&id) == Some(Harness::Opencode);
        if oc {
            core.opencode_keys.insert(id.clone());
        }
        (core.accounts.clone(), oc)
    };
    if oc {
        return opencode_key(shared, &store, &id, key).await;
    }
    let (i, st) = (id.clone(), store.clone());
    let written = tokio::task::spawn_blocking(move || {
        let store = st;
        let h = accounts::mover::key_harness(&store, &i);
        match store.get(h, &i).map(|e| e.account.kind) {
            None => return Err("there is no such account".to_string()),
            Some(Kind::Signin) => return Err("it runs on a sign-in, not on a key".to_string()),
            Some(Kind::Key) => {}
        }
        let file = store.key_file(h, &i);
        keyfile::write(&file, keyfile::word(key.expose())?).map_err(|e| format!("{}: {e}", file.display()))?;
        // the terminal way's last step: the file is looked at (never read) and the account can take agents
        accounts::mover::key_step(&store, &i, coord_types::KeyStep::Set).map(|_| ())
    })
    .await;
    match written {
        Ok(Ok(())) => {}
        Ok(Err(why)) => return not_stored(&why),
        Err(e) => return not_stored(&e.to_string()),
    }
    shared.lock().await.account_key_end(&id, coord_types::KeyStep::Set);
    // an agy key account: agy is asked which models it offers on the key, which says whether it takes it
    if store.harness_of(&id) == Some(Harness::Agy) {
        let checked = account_check(shared, &id, false).await;
        return ok(format!("account {id}: key stored; {}", checked.out.strip_prefix(&format!("account {id}: ")).unwrap_or(&checked.out)));
    }
    ok(format!("account {id}: key stored"))
}

/// Ask Claude Code whether account ID is signed in, record what it says, and say it; with WINDOWS, of a signed-in
/// sign-in account its windows too, read with Claude Code's own `get_usage` ([`account_windows`]): the owner's
/// `Check limits` (0.2 list, line 20) and the check after his sign-in. The check after a refused turn and after a put
/// back asks only whether it is signed in: reading the windows is a special action, run on his press alone (the owner, N-1104 post
/// 586; paper-research, N-1437 post 25: "After a refused turn: `auth status` only"). What is recorded is the core's to
/// act on (`Core::account_checked`): agents move off an account no longer signed in, and those that wait or went on
/// through the gateway look again, as one may be newly signed in.
async fn account_check(shared: &Shared, id: &str, windows: bool) -> Reply {
    use accounts::{mover, signin, Harness};
    let store = shared.lock().await.accounts.clone();
    // an opencode account: opencode is asked, as D23 has it; an OpenCode Go one's windows, on his press, from Go's own
    // usage page
    if store.harness_of(id) == Some(Harness::Opencode) {
        return opencode_check(shared, &store, id, windows).await;
    }
    // an agy account: t3code is asked, or agy on its key
    if store.harness_of(id) == Some(Harness::Agy) {
        return agy_check(shared, &store, id).await;
    }
    // a key account: whether its key file is there, recorded as "signed in" is for the other kind; Claude Code is not
    // asked, as it knows of no login in that folder
    if store.get(Harness::Claude, id).is_some_and(|e| e.account.kind == accounts::Kind::Key) {
        let (store, id, now) = (store.clone(), id.to_string(), mover::now_s());
        let checked = tokio::task::spawn_blocking(move || {
            let set = store.key_set(Harness::Claude, &id)?;
            store.update(Harness::Claude, &id, |s| (s.signed_in, s.signed_in_seen) = (set, now)).map(|_| (id, set))
        })
        .await;
        return match checked {
            Ok(Ok((id, set))) => {
                shared.lock().await.account_checked(&id);
                ok(format!("account {id}: {}", if set { "key set" } else { "no key" }))
            }
            Ok(Err(why)) => err(format!("coord account check: {why}")),
            Err(e) => err(format!("coord account check: {e}")),
        };
    }
    let st = match signin::status(std::path::Path::new(CLAUDE), &store.config(Harness::Claude, id)).await {
        Ok(st) => st,
        Err(why) => return err(format!("coord account check: {why}")),
    };
    let (s2, i2, now) = (store.clone(), id.to_string(), mover::now_s());
    let reply = match tokio::task::spawn_blocking(move || signin::record(&s2, Harness::Claude, &i2, &st, now).map(|state| (st, state))).await {
        Ok(Ok((st, state))) if st.logged_in => {
            let expires = state.login_expires.map(|t| format!(" (its login runs out {})", core::at_secs(t))).unwrap_or_default();
            let window = |name: &str, w: &Option<accounts::Window>, none: &str| w.as_ref().map_or(none.to_string(), |w| format!("{name} {:.0}% until {}", w.used_percent, core::at_secs(w.resets_at)));
            let windows = if !windows {
                String::new()
            } else {
                match account_windows(&store, id).await {
                    Ok(said) => {
                        let five = window("5-hour", &said.five_hour, if said.five_hour_limit == Some(true) { "5-hour nothing used yet" } else { "no 5-hour limit" });
                        format!(" · {five} · {}", window("weekly", &said.seven_day, "weekly not said"))
                    }
                    Err(why) => format!(" · its windows not read: {why}"),
                }
            };
            ok(format!("account {id}: signed in{expires}{windows}"))
        }
        Ok(Ok((st, _))) => ok(format!("account {id}: not signed in{}", st.auth_method.filter(|m| m != "none").map(|m| format!(" (Claude Code says its login is by {m})")).unwrap_or_default())),
        Ok(Err(why)) => return err(format!("coord account check: {why}")),
        Err(e) => return err(format!("coord account check: {e}")),
    };
    shared.lock().await.account_checked(id);
    reply
}

/// CHANGE made to account ID's `state.json` under its lock, off the core's lock (a few small files): what CHANGE gave
/// back, or why nothing was written.
async fn account_update<T: Send + 'static>(store: &accounts::Store, id: &str, change: impl FnOnce(&mut accounts::State) -> T + Send + 'static) -> Result<T, String> {
    let (store, id) = (store.clone(), id.to_string());
    tokio::task::spawn_blocking(move || {
        let mut out = None;
        store.update(accounts::Harness::Claude, &id, |s| out = Some(change(s)))?;
        out.ok_or_else(|| format!("account {id}: not changed"))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

/// Sign-in account ID's look by the window opener, claimed by this coordd in its round (accounts/opener.rs), on what is
/// on file of it (nothing is read for it): its window opened when the gateway's rule says, the opening's own answer
/// recorded (`opener::opened`), and when it is looked at next written back, with what the look found in the journal
/// when that changed. Then it is taken as after a check (`Core::account_checked`): agents on it that it can no longer
/// take move, and those waiting or on the gateway look again.
async fn window_look(shared: Shared, store: accounts::Store, id: String, t: accounts::opener::Times) {
    use accounts::{mover::now_s, opener};
    let config = store.config(accounts::Harness::Claude, &id);
    let journal = |r: Result<Option<String>, String>| match r {
        Ok(Some(found)) => eprintln!("coordd: window opener: account {id}: {found}"),
        Ok(None) => {}
        Err(why) => eprintln!("coordd: window opener: account {id}: not recorded: {why}"),
    };
    let (now, wait) = (now_s(), t.clone());
    // Open: the look's claim stands until the opening is done
    let looked = account_update(&store, &id, move |s| opener::look(s, &wait, now)).await;
    let open = looked.as_ref().is_ok_and(|(_, open)| *open);
    journal(looked.map(|(found, _)| found));
    if open {
        let run = opener::open(std::path::Path::new(CLAUDE), &config, &t).await;
        journal(account_update(&store, &id, move |s| opener::opened(s, run, &t, now_s())).await);
    }
    shared.lock().await.account_checked(&id);
}

/// Sign-in account ID's windows read now with Claude Code's own `get_usage` in its folder (accounts/opener.rs `read`:
/// no prompt, nothing to a model, and nothing of the login through coordd) and recorded as a session's status line
/// would be (`limits::apply`), whether it has a 5-hour limit with them: what it said. On the owner's press only: his
/// `Check limits` and sign-in (`account_check`), and the end of his reset's session (`reset_session`).
async fn account_windows(store: &accounts::Store, id: &str) -> Result<accounts::limits::Said, String> {
    use accounts::{limits, mover::now_s, opener};
    let said = opener::read(std::path::Path::new(CLAUDE), &store.config(accounts::Harness::Claude, id)).await.and_then(|answer| limits::from_usage(&answer))?;
    let (kept, now) = (said.clone(), now_s());
    account_update(store, id, move |s| limits::apply(s, &kept, now)).await?;
    Ok(said)
}

/// The card's `Use` of a reset (`Request::AccountReset`; the owner's only, and the only way coordd has Claude Code
/// use one: the owner, 20 Nov, "DO NOT USE MY LIMIT RESETS"): KIND must be among the resets Claude Code offers account
/// ID now (accounts/resets.rs `offered`), and not one its chooser said it cannot use at this limit (`ResetOffer::why`).
/// The press is claimed in the account's state under its lock, one at a time among his coordds, and its session runs
/// on ([`reset_session`]); the reply says it started.
async fn account_reset(shared: &Shared, who: &Who, id: &str, kind: &str) -> Reply {
    use accounts::{mover::now_s, resets};
    if !matches!(who, Who::Owner) {
        return err("coord account reset: the owner's".into());
    }
    let Some(arg) = resets::argument(kind) else { return err(format!("coord account reset: Claude Code's /limit-reset takes no {kind}")) };
    let store = shared.lock().await.accounts.clone();
    let (i, k, now) = (id.to_string(), kind.to_string(), now_s());
    let claimed = account_update(&store, id, move |s| {
        if let Some(why) = resets::busy(s, now) {
            return Err(format!("{why} on account {i}"));
        }
        match resets::offered(s, now).into_iter().find(|o| o.kind == k) {
            None => return Err(format!("Claude Code offers account {i} no {k} now")),
            Some(coord_types::ResetOffer { why: Some(why), .. }) => return Err(format!("Claude Code says the {k} of account {i} {why}")),
            Some(_) => {}
        }
        s.reset = Some(coord_types::ResetTry { kind: k, since: now, ..Default::default() });
        Ok(())
    })
    .await;
    if let Err(why) | Ok(Err(why)) = claimed {
        return err(format!("coord account reset: {why}"));
    }
    eprintln!("coordd: account {id}: the owner pressed Use on its {kind}; Claude Code applies it in a session of its own");
    shared.lock().await.account_checked(id);
    tokio::spawn(reset_session(shared.clone(), id.to_string(), now, arg));
    ok(format!("account {id}: Claude Code applies the {kind}"))
}

/// Claude Code alone on account ID's folder, started on a terminal of its own named `reset-ID` (roster `account`,
/// `resets::session`; 200 columns, so that its answer stands on one line), for the owner's press or look: the
/// terminal's name and socket, and whether it started. The caller hangs it up (`pty::HANGUP`) when done, whatever came
/// of it.
async fn reset_terminal(store: &accounts::Store, base: &str, coord: &str, id: &str) -> (String, std::path::PathBuf, Result<(), String>) {
    let (h, name) = (accounts::Harness::Claude, format!("reset-{id}"));
    let config = store.config(h, id);
    let mut roster = tokio::process::Command::new(format!("{base}/bin/roster"));
    roster.arg("account").arg(&name).arg(&config).arg(store.dir(h, id)).args(["--cols", "200"]).args(accounts::resets::session(CLAUDE, &config));
    roster.stdin(std::process::Stdio::null()).kill_on_drop(true);
    let socket = coord_types::pty::socket(coord, &name);
    let started = match roster.output().await {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!("its session did not start: {}", last_line(&[o.stderr, o.stdout].concat()))),
        Err(e) => Err(format!("its session did not start: roster account: {e}")),
    };
    (name, socket, started)
}

/// The session of the owner's press at SINCE on account ID (`account_reset`): Claude Code alone on the account's
/// folder ([`reset_terminal`]) is given `/limit-reset ARG` ([`reset_press`]); what it said of how it went is kept as
/// the account's `reset` (a refused kind is offered no more at this limit, one not known to be used stays offered:
/// `resets::went`), its terminal is hung up, the account's windows are read again with `get_usage`, and the card is told.
async fn reset_session(shared: Shared, id: String, since: i64, arg: &'static str) {
    use accounts::resets;
    let (store, base, coord) = {
        let c = shared.lock().await;
        (c.accounts.clone(), c.base.clone(), c.coord.clone())
    };
    let (name, socket, started) = reset_terminal(&store, &base, &coord, &id).await;
    let outcome = match started {
        Ok(()) => reset_press(&coord, &name, &socket, arg, since).await,
        Err(why) => resets::Went::Unknown(why),
    };
    pty_send(&socket, coord_types::pty::HANGUP, &[]).await;
    let done = matches!(outcome, resets::Went::Done(_));
    match &outcome {
        resets::Went::Done(words) => eprintln!("coordd: account {id}: its reset was applied: {words}"),
        resets::Went::Refused(why) => eprintln!("coordd: account {id}: its reset was not applied: {why}"),
        resets::Went::Unknown(why) => eprintln!("coordd: account {id}: its reset is not known to be applied, and may be pressed again: {why}"),
    }
    let kept = account_update(&store, &id, move |s| resets::went(s, since, outcome, accounts::mover::now_s())).await;
    if let Err(why) = kept {
        eprintln!("coordd: account {id}: how its reset went was not kept: {why}");
    }
    if let Err(why) = account_windows(&store, &id).await {
        eprintln!("coordd: account {id}: its windows after the reset are not known: {why}");
    }
    let mut core = shared.lock().await;
    core.account_checked(&id);
    // his reset went through: what is held on a rate limit on it goes on (paper-research N-1446 p894)
    if done {
        core.owner_cleared(&id, "his reset, which went through,");
    }
}

/// `/limit-reset ARG` given to terminal NAME's Claude Code at SOCKET (`pty::COMMAND`: typed once its input box is
/// empty), its screen read every half second until [`accounts::resets::SESSION`] after the press at SINCE and answered
/// as `resets::step` says (one Enter on its confirm, one more only when the first changed nothing, Escape on a
/// question answered no; never text): Claude Code's words once it was applied, or why it was not; not known when its
/// terminal took nothing, ended, or showed nothing known in time.
async fn reset_press(coord: &str, name: &str, socket: &std::path::Path, arg: &str, since: i64) -> accounts::resets::Went {
    use accounts::resets::{self, Shown, Step, Went};
    use coord_types::pty::{COMMAND, KEYS};
    if !pty_send(socket, COMMAND, format!("/limit-reset {arg}").as_bytes()).await {
        return Went::Unknown("its terminal took no command".into());
    }
    let (mut confirm_at, mut last, mut enters, mut seen) = (None, None::<(String, std::time::Instant)>, 0, String::new());
    while accounts::mover::now_s() - since < resets::SESSION {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let (c, n) = (coord.to_string(), name.to_string());
        let Some(screen) = tokio::task::spawn_blocking(move || screen_text(&c, &n)).await.ok().flatten() else {
            return Went::Unknown(format!("its terminal ended before Claude Code said how it went, so {}; its screen showed: {}", resets::MAY_BE_USED, resets::last_lines(&seen)));
        };
        let shown = resets::shown(&screen);
        if matches!(shown, Shown::Confirm { .. }) {
            confirm_at.get_or_insert_with(std::time::Instant::now);
        }
        let confirm_for = confirm_at.map_or(std::time::Duration::ZERO, |t: std::time::Instant| t.elapsed());
        match resets::step(shown, &screen, confirm_for, last.as_ref().map(|(s, t)| (s.as_str(), t.elapsed())), enters, arg) {
            Step::Wait => {}
            Step::Enter => {
                pty_send(socket, KEYS, b"\r").await;
                (last, enters) = (Some((screen.clone(), std::time::Instant::now())), enters + 1);
            }
            Step::End { escape, outcome } => {
                if escape {
                    pty_send(socket, KEYS, b"\x1b").await;
                }
                return Went::of(outcome);
            }
        }
        seen = screen;
    }
    Went::Unknown(format!("Claude Code said nothing coordd knows of how it went within {} s, so {}; its screen showed: {}", resets::SESSION, resets::MAY_BE_USED, resets::last_lines(&seen)))
}

/// The look of the owner's `Check limits` on sign-in account ID, once its check has answered (Q-1441, his yes, 20 Nov:
/// "yeha fine becuase its a processn ow nobdy id therre to process enter"; only on his press, nothing else starts
/// one). Claimed in the account's state under its lock (`resets::busy`: not while a press is applied or another look
/// runs), then, in a task of its own, Claude Code alone on the account's folder ([`reset_terminal`]) lists what the
/// account holds ([`reset_look`]). The list is kept as a notice's is (`resets::noticed`); the terminal is hung up, the
/// claim taken away and the card told. What came of it is in the journal.
async fn resets_look(shared: &Shared, id: &str) {
    use accounts::{mover::now_s, resets};
    let (store, base, coord) = {
        let c = shared.lock().await;
        (c.accounts.clone(), c.base.clone(), c.coord.clone())
    };
    let since = now_s();
    let claimed = account_update(&store, id, move |s| match resets::busy(s, since) {
        Some(why) => Err(why),
        None => {
            s.reset_look = Some(since);
            Ok(())
        }
    })
    .await;
    if let Err(why) | Ok(Err(why)) = claimed {
        return eprintln!("coordd: account {id}: its resets were not read: {why}");
    }
    let (shared, id) = (shared.clone(), id.to_string());
    tokio::spawn(async move {
        let (name, socket, started) = reset_terminal(&store, &base, &coord, &id).await;
        let outcome = match started {
            Ok(()) => reset_look(&coord, &name, &socket, since).await,
            Err(why) => Err(why),
        };
        pty_send(&socket, coord_types::pty::HANGUP, &[]).await;
        match &outcome {
            Ok(resets::Look::Listed(held)) => eprintln!("coordd: account {id}: Claude Code's /limit-reset lists {}", resets::said(held)),
            Ok(found) => eprintln!("coordd: account {id}: Claude Code's /limit-reset answers: {found:?}"),
            Err(why) => eprintln!("coordd: account {id}: its resets were not read: {why}"),
        }
        let kept = account_update(&store, &id, move |s| {
            if s.reset_look == Some(since) {
                s.reset_look = None;
            }
            outcome.ok().and_then(|found| resets::looked(s, found, now_s()))
        })
        .await;
        match kept {
            Ok(Some(note)) => eprintln!("coordd: account {id}: {note}"),
            Ok(None) => {}
            Err(why) => eprintln!("coordd: account {id}: what its /limit-reset lists was not kept: {why}"),
        }
        shared.lock().await.account_checked(&id);
    });
}

/// `/limit-reset` with no kind (`resets::LOOK`) given to terminal NAME's Claude Code at SOCKET, its screen read every
/// half second until [`accounts::resets::SESSION`] after SINCE and answered as `resets::look` says: what it lists, once
/// Escape has answered its list (which keeps them all), or the words it answered with instead, at once (never `Wait`);
/// Err when its terminal said neither. Only that command and Escape are sent: never Enter, never text.
async fn reset_look(coord: &str, name: &str, socket: &std::path::Path, since: i64) -> Result<accounts::resets::Look, String> {
    use accounts::resets::{self, Look};
    use coord_types::pty::{COMMAND, KEYS};
    if !pty_send(socket, COMMAND, resets::LOOK.as_bytes()).await {
        return Err("its terminal took no command".into());
    }
    let mut seen = String::new();
    while accounts::mover::now_s() - since < resets::SESSION {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let (c, n) = (coord.to_string(), name.to_string());
        let Some(screen) = tokio::task::spawn_blocking(move || screen_text(&c, &n)).await.ok().flatten() else {
            return Err(format!("its terminal ended before Claude Code listed its resets; its screen showed: {}", resets::last_lines(&seen)));
        };
        match resets::look(&screen, accounts::mover::now_s()) {
            Look::Wait => seen = screen,
            Look::Listed(held) => {
                pty_send(socket, KEYS, b"\x1b").await;
                return Ok(Look::Listed(held));
            }
            said => return Ok(said),
        }
    }
    Err(format!("Claude Code listed no resets within {} s; its screen showed: {}", resets::SESSION, resets::last_lines(&seen)))
}

/// A frame of KIND with BYTES (`coord_types::pty`) to the terminal holder at SOCKET: whether it was written.
async fn pty_send(socket: &std::path::Path, kind: u8, bytes: &[u8]) -> bool {
    match UnixStream::connect(socket).await {
        Ok(mut c) => c.write_all(&coord_types::pty::frame(kind, bytes)).await.is_ok(),
        Err(_) => false,
    }
}

/// Account ID of H was signed in anew by its harness's own sign-in: its login's refusals before count no more, nor is
/// it out for them ([`accounts::State::signed_in_again`]; for an opencode account, a refused sign-in or key,
/// `Core::opencode_said`).
async fn signed_in_anew(shared: &Shared, h: accounts::Harness, id: &str) {
    let (store, i) = (shared.lock().await.accounts.clone(), id.to_string());
    if let Ok(Err(why)) | Err(why) = tokio::task::spawn_blocking(move || store.update(h, &i, accounts::State::signed_in_again).map(|_| ())).await.map_err(|e| e.to_string()) {
        eprintln!("coordd: account {id}: signed in again, but its refused login is still on file: {why}");
    }
}

/// Sign-in S of account ID has ended, by the browser's redirect, a pasted code, a cancel or a failure: what Claude
/// Code now says of the account is recorded, and the handle let go unless a newer sign-in took its place.
async fn signin_ended(shared: Shared, id: String, s: Arc<accounts::signin::SignIn>) {
    let ended = s.wait().await;
    if ended == accounts::signin::Ended::Ok {
        signed_in_anew(&shared, accounts::Harness::Claude, &id).await;
    }
    let r = account_check(&shared, &id, true).await;
    eprintln!("coordd: account {id}: its sign-in ended ({}); {}", match ended { accounts::signin::Ended::Ok => "done".to_string(), accounts::signin::Ended::Failed(why) => why }, r.out);
    let mut core = shared.lock().await;
    if core.signins.get(&id).is_some_and(|x| Arc::ptr_eq(x, &s)) {
        core.signins.remove(&id);
        // its cgroup, now empty; one a later sign-in of the account already runs in stays (it is not empty)
        let _ = std::fs::remove_dir(cgroup::root().join(signin_group(&id)));
    }
}

/// The requests that change the accounts' store (accounts/mod.rs), each in three steps so that the store's lock,
/// which another of the owner's coordds may hold, is never waited for under the core's lock: what the core knows is
/// taken under its lock; the store's files are read and written by a task of their own, which waits for the store's
/// lock as long as its holder keeps it; the core takes the result under its lock again.
///
/// - `AccountFor` (roster, at each start of a Claude Code or opencode agent) and `MoveAgent`: the choice of account
///   (`mover::Choosing`).
/// - `AccountEdit` (and `AccountTier`, the edit of a tier alone) and `AccountClear`: the owner's change of one account's
///   own files.
/// - `AccountKey`: coordc's three questions about a key account's key file (`mover::key_step`).
/// - `AccountKeyRemove`: the window's `Remove key`, where coordd deletes the file itself.
async fn account_store(shared: &Shared, who: &Who, req: Request) -> Reply {
    use accounts::Harness;
    /// F run where it may wait, its panic an Err.
    async fn off<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
        tokio::task::spawn_blocking(f).await.map_err(|e| format!("the accounts' store: {e}"))
    }
    // `coord account tier ID N` is the edit of a tier alone: one arm answers both
    let req = match req {
        Request::AccountTier { id, tier } => Request::AccountEdit { id, tier: Some(tier), label: None, off: None },
        req => req,
    };
    match req {
        Request::AccountFor { name } => {
            // a Claude Code agent whose process coordd's going down ended (`Agent::down_cut`), and that runs no more:
            // its transcript, read off the lock, says whether that end cut a turn, which its start then goes on with
            let transcript = {
                let core = shared.lock().await;
                core.agents.get(&name).filter(|a| a.down_cut && a.kind == "claude" && !a.running()).and_then(|a| a.transcript())
            };
            let down_cut = match transcript {
                Some(t) => off(move || follow::cut_at(&t)).await.ok().flatten(),
                None => None,
            };
            let choosing = {
                let mut core = shared.lock().await;
                if core.agents.get(&name).is_some_and(|a| a.native) {
                    telemetry_up(&mut core).await;
                }
                match core.account_for_begin(who, &name, down_cut) {
                    Ok(c) => c,
                    Err(answered) => return answered,
                }
            };
            let answer = match off(move || choosing.run()).await {
                Ok(chosen) => shared.lock().await.account_for_end(chosen),
                Err(why) => return err(format!("coord account: {why}")),
            };
            // an agy agent on an account t3code signed in: its thread is on the account's t3code server, which is to be up
            let (store, coord_id, agy) = {
                let core = shared.lock().await;
                (core.accounts.clone(), core.coord_id(), core.agents.get(&name).is_some_and(|a| a.kind == "agy"))
            };
            if answer.ok && agy {
                let bound = store
                    .bound_to(Harness::Agy, &coord_id, &name)
                    .filter(|e| e.account.kind == accounts::Kind::Signin && accounts::agy::legacy(&store.dir(Harness::Agy, &e.id)));
                if let Some(e) = bound
                    && let Err(why) = agy_server_up(shared, &store, &e.id).await
                {
                    return err(format!("coord account: {name}'s account {}: its t3code server is not up: {why}", e.id));
                }
            }
            answer
        }
        Request::MoveAgent { name, to } => {
            let choosing = match shared.lock().await.move_begin(who, &name, to.as_deref()) {
                Ok(c) => c,
                Err(answered) => return answered,
            };
            match off(move || choosing.run()).await {
                Ok(chosen) => shared.lock().await.move_end(chosen),
                Err(why) => err(format!("coord account move: {why}")),
            }
        }
        Request::AccountEdit { id, tier, label, off: turned } => {
            let what = if label.is_none() && turned.is_none() { "tier" } else { "edit" };
            let store = match shared.lock().await.account_owners(who, what) {
                Ok(store) => store,
                Err(answered) => return answered,
            };
            if tier.is_none() && label.is_none() && turned.is_none() {
                return err("coord account edit: nothing to change".into());
            }
            let label = match label.map(|l| l.trim().to_string()) {
                Some(l) if l.is_empty() || l.chars().any(char::is_control) => return err(format!("coord account {what}: a label is words on one line")),
                l => l,
            };
            let i = id.clone();
            let (t, l) = (tier, label.clone());
            let edited = off(move || {
                let h = store.harness_of(&i).unwrap_or(Harness::Claude);
                if t.is_some() || l.is_some() {
                    store.set_account(h, &i, |a| {
                        if let Some(t) = t {
                            a.tier = t;
                        }
                        if let Some(l) = l {
                            a.label = l;
                        }
                    })?;
                }
                // off: since now, or since it was turned off before; on: the mark goes
                if let Some(o) = turned {
                    store.update(h, &i, |s| s.off = if o { s.off.or(Some(accounts::mover::now_s())) } else { None })?;
                }
                Ok::<_, String>(h)
            });
            match edited.await.and_then(|r| r) {
                Ok(h) => {
                    let mut said: Vec<String> = Vec::new();
                    said.extend(tier.map(|t| format!("tier {t}")));
                    said.extend(label.map(|l| format!("label “{l}”")));
                    match turned {
                        // its agents leave at the end of their turn: this coord's are marked now, the others' at their round
                        Some(true) => {
                            let st = shared.lock().await.accounts.clone();
                            if let Ok(seen) = off(move || accounts::mover::seen(&st)).await {
                                shared.lock().await.accounts_seen(&seen);
                            }
                            said.push(match h {
                                Harness::Opencode | Harness::Agy => "off: it takes no agent; one that runs on it keeps it until it stops".into(),
                                _ => "off: it takes no agent; its agents move at the end of their turn".into(),
                            });
                        }
                        // who waits for an account is tried again now
                        Some(false) => {
                            shared.lock().await.account_cleared();
                            said.push("on: it takes agents again".into());
                        }
                        None => {}
                    }
                    ok(format!("account {id}: {}", said.join(", ")))
                }
                Err(why) => err(format!("coord account {what}: {why}")),
            }
        }
        Request::AccountClear { id } => {
            let store = match shared.lock().await.account_owners(who, "clear") {
                Ok(store) => store,
                Err(answered) => return answered,
            };
            let i = id.clone();
            match off(move || store.update(store.harness_of(&i).unwrap_or(Harness::Claude), &i, accounts::State::cleared)).await.and_then(|r| r) {
                Ok(_) => {
                    shared.lock().await.owner_cleared(&id, "his press, it can take requests,");
                    ok(format!("account {id}: can take requests again"))
                }
                Err(why) => err(format!("coord account clear: {why}")),
            }
        }
        Request::AccountKey { id, step } => {
            let store = match shared.lock().await.account_key_begin(who) {
                Ok(store) => store,
                Err(answered) => return answered,
            };
            let i = id.clone();
            match off(move || accounts::mover::key_step(&store, &i, step)).await.and_then(|r| r) {
                Ok(answer) => {
                    shared.lock().await.account_key_end(&id, step);
                    ok(answer)
                }
                Err(why) => err(format!("coord account key: {why}")),
            }
        }
        // the window's `Remove key`: coordd deletes the file (a window on another machine cannot), then the same
        // last step as coordc's `--remove`
        Request::AccountKeyRemove { id } => {
            let store = match shared.lock().await.account_key_begin(who) {
                Ok(store) => store,
                Err(_) => return err("account key: the owner's window only".into()),
            };
            let i = id.clone();
            let removed = off(move || {
                store.key_remove(accounts::mover::key_harness(&store, &i), &i)?;
                accounts::mover::key_step(&store, &i, coord_types::KeyStep::Removed)
            });
            match removed.await.and_then(|r| r) {
                Ok(answer) => {
                    shared.lock().await.account_key_end(&id, coord_types::KeyStep::Removed);
                    ok(answer)
                }
                Err(why) => err(format!("account key: {why}")),
            }
        }
        _ => err("coordd: not a change of the accounts' store".into()),
    }
}

/// `coord account remove ID` and the card's Remove (`Request::AccountRemove`), and with UNDO the card's `Keep it`
/// (`Request::AccountKeep`), off the core's lock. An id that is no
/// folder name of the store's is refused before anything else (docket-core3, N-684 post 6: no sign-out runs outside
/// the store).
///
/// An account no agent runs on is removed now ([`account_set_aside`]). One that agents run on is not refused
/// (paper-research, N-591 post 24): it is marked (`State::leaving`), so it takes no agent from then on; each coordd
/// moves its own agents off it at the end of their turn, onto another account or, where none can take them and the
/// coord has the gateway, on through the gateway ([`Core::accounts_seen`], at once here and at each round
/// elsewhere); and the first coordd whose round finds it free removes it ([`account_left`]). Refused in words only
/// when its agents would have nowhere to go: no other account can take one now and this coord has no gateway. With
/// UNDO the mark is taken away again, and the agents that moved meanwhile stay where they went.
///
/// An opencode account that a running agent runs on is refused in words: its agent keeps its server, and with it the
/// account's folders, until it stops, and nothing moves it off.
async fn account_remove(shared: &Shared, who: &Who, id: &str, undo: bool) -> Reply {
    use accounts::{mover, Harness};
    if !matches!(who, Who::Owner) {
        return err("coord account remove: the owner's".into());
    }
    if let Some(why) = accounts::Store::bad_id(id) {
        return err(format!("coord account remove: {why}"));
    }
    let (store, gateway) = {
        let core = shared.lock().await;
        (core.accounts.clone(), core.gateway())
    };
    let h = store.harness_of(id).unwrap_or(Harness::Claude);
    let Some(entry) = store.get(h, id) else { return err(format!("coord account remove: no account {id}")) };
    let mark = |leaving: Option<i64>| {
        let (st, i) = (store.clone(), id.to_string());
        async move { tokio::task::spawn_blocking(move || st.update(h, &i, |s| s.leaving = leaving).map(|_| ())).await.map_err(|e| e.to_string()).and_then(|r| r) }
    };
    if undo {
        if entry.state.leaving.is_none() {
            return ok(format!("account {id}: it is not being removed"));
        }
        if let Err(why) = mark(None).await {
            return err(format!("coord account remove: {why}"));
        }
        eprintln!("coordd: account {id}: kept: the owner took its removal back");
        shared.lock().await.account_cleared();
        return ok(format!("account {id}: kept; the agents that moved meanwhile stay where they went"));
    }
    let mut on = store.running_on(h, id);
    // an opencode or agy agent runs on its account's folders for as long as its session runs, and nothing moves it
    // off: the account is not removed under it. A binding of this coord's that no running agent holds is let go first.
    if h != Harness::Claude && !on.is_empty() {
        let (coord, idle): (String, Vec<String>) = {
            let core = shared.lock().await;
            let coord = core.coord_id();
            let idle = on.iter().filter(|b| b.coord == coord && !core.agents.get(&b.agent).is_some_and(|a| a.running())).map(|b| b.agent.clone()).collect();
            (coord, idle)
        };
        for name in &idle {
            store.unbind_from(h, id, &coord, name);
        }
        on.retain(|b| b.coord != coord || !idle.contains(&b.agent));
        if !on.is_empty() {
            let names = on.iter().map(|b| format!("{}.{}", b.coord, b.agent)).collect::<Vec<_>>().join(", ");
            let (run, they) = if on.len() == 1 { ("runs", "it is") } else { ("run", "they are") };
            return err(format!("coord account remove: {names} {run} on it; it can be removed once {they} stopped"));
        }
    }
    if on.is_empty() {
        return match account_set_aside(shared, &store, h, id, false).await {
            Ok(said) => ok(said),
            Err(why) => err(format!("coord account remove: {why}")),
        };
    }
    let names = on.iter().map(|b| format!("{}.{}", b.coord, b.agent)).collect::<Vec<_>>().join(", ");
    let (run, they, their) = if on.len() == 1 { ("runs", "it", "its") } else { ("run", "them", "their") };
    if entry.state.leaving.is_some() {
        return ok(format!("account {id}: being removed already; {names} still {run} on it"));
    }
    let now = mover::now_s();
    let other = store.list(h).iter().any(|e| e.id != id && e.state.can_take(now));
    // said before anything is changed: a click of his leaves no agent standing
    if !other && !gateway {
        return err(format!("coord account remove: {names} {run} on it and no other account can take {they} now; sign in to another account first"));
    }
    if let Err(why) = mark(Some(now)).await {
        return err(format!("coord account remove: {why}"));
    }
    // a sign-in of this coordd's that runs for it has nothing to sign in to any more
    signins_end(shared, id).await;
    let to = if other { "another account" } else { "the gateway" };
    eprintln!("coordd: account {id}: to be removed when no agent runs on it; {names} to {to} at the end of {their} turn");
    // this coord's agents on it are marked now; the other coordds' at their round
    let st = store.clone();
    if let Ok(seen) = tokio::task::spawn_blocking(move || mover::seen(&st)).await {
        shared.lock().await.accounts_seen(&seen);
    }
    ok(format!("account {id}: {names} {} to {to} at the end of {their} turn; then it is removed", if on.len() == 1 { "moves" } else { "move" }))
}

/// Account ID is set aside and signed out, the removal itself, for the click on an account no agent runs on and
/// for [`account_left`]. A sign-in of this coordd's that runs for it is ended; the folder is set aside whole
/// ([`accounts::Store::remove`]: under the account's lock, and refused there while an agent is bound to it); and
/// only then Claude Code signs the account out itself, in the set-aside folder (`claude auth logout` under it;
/// coordd reads and deletes nothing of the login): a removal that is refused never touches the login. A sign-out
/// that fails (no network) is said in the answer and the journal, and the set-aside folder keeps its mark, so the
/// page's Removed line says "not signed out" ([`accounts::Store::signed_out`] takes the mark away only when Claude
/// Code has answered); the folder is set aside all the same, where no agent is sent, with whatever Claude Code left
/// in it. The Ok is the answer in words.
///
/// With LEFT it is the round's removal of an account that was being removed: made only if the account is still
/// being removed when its lock is held ([`accounts::Store::remove_left`]), so a `Keep it` clicked since the round
/// looked wins and nothing is touched. No sign-in is ended then: none runs for an account that is being removed
/// (ended at the click, refused since), and one that runs was started after a `Keep it`.
///
/// An opencode account (H) is signed out by opencode's own `auth logout PROVIDER`, with the set-aside folder's
/// `home/` as its folders, in the same way.
async fn account_set_aside(shared: &Shared, store: &accounts::Store, h: accounts::Harness, id: &str, left: bool) -> Result<String, String> {
    use accounts::{opencode, signin, Harness};
    if !left {
        signins_end(shared, id).await;
    }
    let provider = store.get(h, id).and_then(|e| e.account.provider);
    let kind = store.kind(h, id).unwrap_or_default();
    let (st, i, stamp) = (store.clone(), id.to_string(), chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());
    let aside = tokio::task::spawn_blocking(move || if left { st.remove_left(h, &i, &stamp) } else { st.remove(h, &i, &stamp) }).await.map_err(|e| e.to_string())??;
    // the folder is no account's any more: no agent is bound to it and none is sent there
    let signed_out = match (h, provider) {
        (Harness::Claude, _) => signin::logout(std::path::Path::new(CLAUDE), &aside.join("config")).await,
        (Harness::Opencode, Some(p)) => match opencode_program().await {
            Ok(program) => opencode::logout(&program, &aside, &p).await,
            Err(why) => Err(why),
        },
        (Harness::Opencode, None) => Err("it named no provider to sign out of".into()),
        (Harness::Agy, _) => agy_sign_out(shared, &aside, kind).await,
    };
    let who = h.signs_out_of(&aside);
    Ok(match signed_out {
        Ok(()) => {
            store.signed_out(&aside);
            eprintln!("coordd: account {id}: removed: its folder set aside at {}, signed out there by {who}", aside.display());
            format!("account {id}: removed (signed out; its folder is set aside at {})", aside.display())
        }
        Err(why) => {
            eprintln!("coordd: account {id}: removed: its folder set aside at {}; {who}'s sign-out there failed ({why})", aside.display());
            format!("account {id}: removed, but {who}'s sign-out failed: {why}. Its folder, with what {who} left in it, is set aside at {}", aside.display())
        }
    })
}

/// End the sign-in of account ID that runs in this coordd, Claude Code's, opencode's or t3code's, if one does.
async fn signins_end(shared: &Shared, id: &str) {
    let (claude, opencode, agy) = {
        let core = shared.lock().await;
        (
            core.signins.get(id).filter(|s| s.ended().is_none()).cloned(),
            core.opencode_runs.get(id).filter(|r| r.ended().is_none()).cloned(),
            core.agy_signins.get(id).filter(|r| r.ended().is_none()).cloned(),
        )
    };
    if let Some(s) = claude {
        s.cancel().await;
    }
    if let Some(r) = opencode {
        r.cancel().await;
    }
    if let Some(r) = agy {
        r.cancel().await;
    }
}

/// Account ID, which the owner asked to remove while agents ran on it, has none left by this coordd's round: it is
/// removed now ([`account_set_aside`]). Another of the owner's coordds may have done it first, or an agent may have
/// been bound again since the round looked: then there is nothing to say, and the next round looks again.
async fn account_left(shared: Shared, store: accounts::Store, id: String) {
    let h = store.harness_of(&id).unwrap_or(accounts::Harness::Claude);
    match account_set_aside(&shared, &store, h, &id, true).await {
        Ok(_) => shared.lock().await.account_cleared(),
        Err(why) if why.starts_with("no account ") || why.contains(" still run") || why.ends_with(": not being removed") => {}
        Err(why) => eprintln!("coordd: account {id}: its removal, which waited for its agents, did not happen: {why}"),
    }
}

/// `Request::AccountPutBack`: the removed account whose set-aside folder is NAME is an account again
/// ([`accounts::Store::put_back`], off the core's lock), and what it now is is asked as `coord account check` asks it:
/// of a sign-in account, Claude Code's own answer (the removal signed it out, unless that failed); of a key account,
/// whether its key file is there. Agents that wait for an account are tried.
async fn account_put_back(shared: &Shared, who: &Who, name: &str) -> Reply {
    if !matches!(who, Who::Owner) {
        return err("account put back: the owner's".into());
    }
    let store = shared.lock().await.accounts.clone();
    let (st, n) = (store.clone(), name.to_string());
    let h = accounts::Harness::ALL.into_iter().find(|h| store.removed(*h).iter().any(|r| r.name == name)).unwrap_or(accounts::Harness::Claude);
    let id = match tokio::task::spawn_blocking(move || st.put_back(h, &n)).await {
        Ok(Ok(id)) => id,
        Ok(Err(why)) => return err(format!("account put back: {why}")),
        Err(e) => return err(format!("account put back: {e}")),
    };
    let checked = account_check(shared, &id, false).await;
    eprintln!("coordd: account {id}: put back from {name}: {}", checked.out);
    // for the card: put back, and (the removal signed it out) to be signed in again
    let (st, i, now) = (store.clone(), id.clone(), accounts::mover::now_s());
    if let Ok(Err(why)) | Err(why) = tokio::task::spawn_blocking(move || st.update(h, &i, |s| s.put_back = (!s.signed_in).then_some(now)).map(|_| ())).await.map_err(|e| e.to_string()) {
        eprintln!("coordd: account {id}: that it was put back is not on file: {why}");
    }
    shared.lock().await.account_cleared();
    if checked.ok { ok(format!("account {id}: put back; {}", checked.out.strip_prefix(&format!("account {id}: ")).unwrap_or(&checked.out))) } else { ok(format!("account {id}: put back; whether it is signed in is not known: {}", checked.out)) }
}

/// The cgroup (under coordd's root) the sign-in of account ID runs in.
fn signin_group(id: &str) -> String {
    format!("signin/{id}")
}

/// The pinned opencode of this coordd's coord (`accounts::opencode::program`), or why there is none or it is not the
/// one fetched. Its sha256 is read off the async threads, at every call: each run is checked.
async fn opencode_program() -> Result<std::path::PathBuf, String> {
    let coord = std::path::PathBuf::from(std::env::var_os("COORD").unwrap_or_default());
    tokio::task::spawn_blocking(move || accounts::opencode::program(&coord)).await.map_err(|e| format!("opencode: its check failed: {e}"))?
}

/// Start opencode's own `auth login -p PROVIDER [-m METHOD]` for opencode account ID, in the account's folders, on a
/// terminal of its own and in a cgroup of its own, as the account's one running sign-in. The run, or why none was
/// started: a second sign-in of one account is refused in words, since both would write the same file.
async fn opencode_start(shared: &Shared, store: &accounts::Store, id: &str, method: Option<&str>) -> Result<Arc<accounts::opencode::Run>, String> {
    use accounts::{opencode, Harness};
    let program = opencode_program().await?;
    let entry = store.get(Harness::Opencode, id).ok_or_else(|| format!("no account {id}"))?;
    if entry.state.leaving.is_some() {
        return Err(format!("account {id} is being removed"));
    }
    let provider = entry.account.provider.ok_or_else(|| format!("account {id} names no provider"))?;
    if let Some(why) = opencode::bad_provider(&provider) {
        return Err(why);
    }
    store.layout(Harness::Opencode, id)?;
    let runs = |core: &Core| core.opencode_runs.get(id).is_some_and(|r| r.ended().is_none());
    if runs(&*shared.lock().await) {
        return Err(format!("a sign-in of account {id} runs already"));
    }
    let mut args = vec!["auth", "login", "-p", provider.as_str()];
    if let Some(m) = method {
        args.extend(["-m", m]);
    }
    let run = Arc::new(opencode::Run::start(&program, &store.dir(Harness::Opencode, id), &args)?);
    // in a cgroup of its own, neither coordd's nor an agent's, as Claude Code's sign-in; one that cannot be put there
    // is not run
    if let Err(e) = cgroup::place(run.pid(), &signin_group(id)) {
        run.cancel().await;
        return Err(format!("its run could not be put in a cgroup of its own ({}): {e}", cgroup::root().join(signin_group(id)).display()));
    }
    let mut core = shared.lock().await;
    if runs(&core) {
        drop(core);
        run.cancel().await;
        return Err(format!("a sign-in of account {id} runs already"));
    }
    core.opencode_runs.insert(id.to_string(), run.clone());
    drop(core);
    tokio::spawn(opencode_ended(shared.clone(), id.to_string(), run.clone()));
    Ok(run)
}

/// `Request::AccountKeyStore` on opencode account ID (the owner's alone, checked by the caller): what he pasted goes
/// into opencode's own sign-in and nowhere else. opencode's `auth login` for the account's provider is started, and
/// KEY is written once into its masked `Enter your API key` prompt (`Run::type_key`: only that prompt, only on a
/// terminal that does not echo; `Run::give_key`). Any other prompt, or none, gets nothing, and the run is ended; the
/// owner's Cancel (`AccountCancel`) ends it while it waits, and nothing is typed. KEY is dropped in every case, which
/// overwrites its bytes. Then `opencode auth list` says whether opencode keeps a sign-in for the account. The answer
/// has no word of the key.
///
/// An OpenCode Go account's key, once opencode keeps a sign-in by it, is also kept by coordd to read Go's usage with
/// and for nothing else (`opencode::keep_usage_key`; the owner, N-1104 p808: "A is fine . yes"), and Go's usage is
/// read at once ([`go_read`]); the answer says how that went after `key stored`.
///
/// ID is in `opencode_keys` when this is called: `account_key_store` put it there in the take of the lock that found
/// the account opencode's.
async fn opencode_key(shared: &Shared, store: &accounts::Store, id: &str, key: coord_types::Secret) -> Reply {
    use accounts::{opencode, signin::Ended, Harness};
    let not_stored = |why: &str| err(format!("account {id}: key not stored: {why}"));
    let usage_key = (store.get(Harness::Opencode, id).and_then(|e| e.account.provider).as_deref() == Some(opencode::GO)).then(|| key.clone());
    // a Cancel before the run is in `opencode_runs` (opencode's check and start take a moment) takes the account out
    // of `opencode_keys`: the run is then ended as it starts
    let started = opencode_start(shared, store, id, None).await;
    let cancelled = !shared.lock().await.opencode_keys.remove(id);
    let run = match started {
        Ok(r) => r,
        Err(why) => return not_stored(&why),
    };
    if cancelled {
        run.cancel().await;
    }
    // the run is the account's one sign-in in `opencode_runs`, so `AccountCancel` ends it while it waits here
    let ended = match run.give_key(key).await {
        Ok(ended) => ended,
        Err(why) => return not_stored(&why),
    };
    let checked = opencode_check(shared, store, id, false).await;
    match (checked.ok && checked.out.contains(": signed in"), ended) {
        (true, _) => {
            let usage = match usage_key.map(|k| opencode::keep_usage_key(store, id, &k)) {
                Some(Ok(())) => match go_read(shared, store, id).await {
                    Ok(read) => format!("; {read}"),
                    Err(why) => format!("; kept for Go's usage, which was not read: {why}"),
                },
                Some(Err(why)) => format!("; not kept for Go's usage: {why}"),
                None => String::new(),
            };
            ok(format!("account {id}: key stored{usage}"))
        }
        (false, Ended::Failed(why)) => not_stored(&why),
        (false, Ended::Ok) if checked.ok => not_stored(&format!("opencode keeps no sign-in for it afterwards: {}", run.shown().trim())),
        (false, Ended::Ok) => not_stored(&checked.out),
    }
}

/// The sign-in requests on opencode account ID (`account_signin` for an opencode account), off the core's lock:
/// `AccountSignin` starts opencode's `auth login` with the way METHOD and answers its link and its instructions;
/// `AccountCode` hands it the code the browser showed; `AccountCancel` ends it; `AccountCheck` asks `opencode auth
/// list` and records it. A run's end, however it comes, is recorded by `opencode_ended`.
async fn opencode_signin(shared: &Shared, who: &Who, store: &accounts::Store, id: &str, req: Request) -> Reply {
    use accounts::opencode;
    let running = shared.lock().await.opencode_runs.get(id).filter(|r| r.ended().is_none()).cloned();
    match req {
        Request::AccountSignin { method, .. } => {
            let Some(method) = method.filter(|m| !m.trim().is_empty()) else {
                return err("coord account signin: an opencode account signs in by a way opencode names, or by its key in the API key field".into());
            };
            // the ChatGPT browser way waits for the redirect on Codex's own port: while another holds it, nothing starts
            if method == opencode::ON_CODEX_PORT && running.is_none() && !opencode::codex_port_free() {
                return err(format!("coord account signin: port {} on this machine is in use (a Codex sign-in waits on it), so this way could not get the browser's answer; nothing was started", opencode::CODEX_PORT));
            }
            let run = match running {
                Some(r) => r,
                None => match opencode_start(shared, store, id, Some(method.as_str())).await {
                    Ok(r) => r,
                    Err(why) => return err(format!("coord account signin: {why}")),
                },
            };
            match run.link().await {
                Ok((url, Some(said))) => ok(format!("{url}\n{said}")),
                Ok((url, None)) => ok(url),
                Err(why) => err(format!("coord account signin: {why}")),
            }
        }
        Request::AccountCode { code, .. } => {
            let Some(run) = running else { return err(format!("coord account code: no sign-in runs for {id}")) };
            if let Err(why) = run.paste(&code).await {
                return err(format!("coord account code: {why}"));
            }
            run.wait().await;
            opencode_check(shared, store, id, true).await
        }
        Request::AccountCancel { .. } => {
            // looked at under one lock with the key's start (`opencode_key`): the run that runs, or a key's run not
            // in `opencode_runs` yet, which is then ended as it starts
            let (running, waiting) = {
                let mut core = shared.lock().await;
                (core.opencode_runs.get(id).filter(|r| r.ended().is_none()).cloned(), core.opencode_keys.remove(id))
            };
            match running {
                Some(r) => {
                    r.cancel().await;
                    ok(format!("account {id}: its sign-in is ended"))
                }
                None if waiting => ok(format!("account {id}: its sign-in is ended")),
                None => ok(format!("account {id}: no sign-in runs")),
            }
        }
        _ => {
            let r = opencode_check(shared, store, id, matches!(who, Who::Owner)).await;
            // a running sign-in's words are the owner's; an agent learns only whether it is signed in
            match running.filter(|_| matches!(who, Who::Owner)) {
                Some(run) if r.ok => ok(format!("{}\na sign-in runs for it; its words so far:\n{}", r.out, run.shown())),
                _ => r,
            }
        }
    }
}

/// Ask opencode what it keeps for opencode account ID (`opencode auth list` in its folders), and, while it keeps a
/// sign-in, which models it offers there for the account's provider (`opencode models PROVIDER`; a failure of that is
/// said in coordd's log and keeps the models it had); record both, and say it. With USAGE (the owner's `Check limits`
/// and sign-in, as Claude Code's windows are read on his press: `account_check`), an OpenCode Go account signed in with
/// its key kept for its usage has Go's usage read as well ([`go_read`]).
async fn opencode_check(shared: &Shared, store: &accounts::Store, id: &str, usage: bool) -> Reply {
    use accounts::{mover, opencode, Harness};
    let program = match opencode_program().await {
        Ok(p) => p,
        Err(why) => return err(format!("coord account check: {why}")),
    };
    let dir = store.dir(Harness::Opencode, id);
    let st = match opencode::status(&program, &dir).await {
        Ok(st) => st,
        Err(why) => return err(format!("coord account check: {why}")),
    };
    let models = match store.get(Harness::Opencode, id).and_then(|e| e.account.provider).filter(|_| !st.stored.is_empty()) {
        Some(provider) => match opencode::models(&program, &dir, &provider).await {
            Ok(m) => Some(m),
            Err(why) => {
                eprintln!("coordd: account {id}: its models were not read: {why}");
                None
            }
        },
        None => None,
    };
    let kept: Vec<String> = st.stored.iter().map(|(name, kind)| format!("{name} ({kind})")).collect();
    let (st_, i, now) = (store.clone(), id.to_string(), mover::now_s());
    let recorded = tokio::task::spawn_blocking(move || opencode::record(&st_, &i, &st, models, now)).await;
    // an OpenCode Go account signed in, with its key kept for it: Go's usage too
    let go = usage && store.get(Harness::Opencode, id).is_some_and(|e| e.account.provider.as_deref() == Some(opencode::GO)) && store.usage_key_set(id);
    let usage = match (&recorded, go) {
        (Ok(Ok(state)), true) if state.signed_in => match go_read(shared, store, id).await {
            Ok(read) => format!("; {read}"),
            Err(why) => format!("; Go's usage was not read: {why}"),
        },
        _ => String::new(),
    };
    match recorded {
        Ok(Ok(state)) if state.signed_in => ok(format!("account {id}: signed in (opencode keeps {}){usage}", kept.join(", "))),
        Ok(Ok(_)) => ok(format!("account {id}: not signed in")),
        Ok(Err(why)) => err(format!("coord account check: {why}")),
        Err(e) => err(format!("coord account check: {e}")),
    }
}

/// OpenCode Go's usage page read for account ID with the key coordd keeps for it (`opencode::go_usage`: the read uses
/// nothing of the plan) and recorded as its meters say (`limits::apply`, from `limits::GO_USAGE`): the 5-hour, weekly
/// and monthly windows, and the account out until the last reset of one used up. Whoever waits for an account looks
/// again (`Core::account_checked`). The answer names the windows read.
async fn go_read(shared: &Shared, store: &accounts::Store, id: &str) -> Result<String, String> {
    use accounts::{limits, mover, opencode, Harness};
    let said = opencode::go_usage(std::path::Path::new("curl"), &store.usage_key_file(id)).await?;
    let window = |name: &str, w: &Option<accounts::Window>| w.as_ref().map(|w| format!("{name} {:.0}% until {}", w.used_percent, core::at_secs(w.resets_at)));
    let read: Vec<String> = [window("5-hour", &said.five_hour), window("weekly", &said.seven_day), window("monthly", &said.month)].into_iter().flatten().collect();
    let (st, i, now) = (store.clone(), id.to_string(), mover::now_s());
    tokio::task::spawn_blocking(move || st.update(Harness::Opencode, &i, |s| limits::apply(s, &said, now))).await.map_err(|e| e.to_string())??;
    shared.lock().await.account_checked(id);
    Ok(format!("Go's usage: {}", read.join(" · ")))
}

/// opencode's sign-in RUN of account ID has ended, by the browser, a pasted code, a key, a cancel or a failure: what
/// opencode now keeps is recorded, and the handle let go unless a newer run took its place.
async fn opencode_ended(shared: Shared, id: String, run: Arc<accounts::opencode::Run>) {
    let ended = run.wait().await;
    if ended == accounts::signin::Ended::Ok {
        signed_in_anew(&shared, accounts::Harness::Opencode, &id).await;
    }
    let store = shared.lock().await.accounts.clone();
    let r = opencode_check(&shared, &store, &id, false).await;
    eprintln!("coordd: account {id}: its opencode sign-in ended ({}); {}", match ended { accounts::signin::Ended::Ok => "done".to_string(), accounts::signin::Ended::Failed(why) => why }, r.out);
    let mut core = shared.lock().await;
    if core.opencode_runs.get(&id).is_some_and(|x| Arc::ptr_eq(x, &run)) {
        core.opencode_runs.remove(&id);
        let _ = std::fs::remove_dir(cgroup::root().join(signin_group(&id)));
    }
}

/// agent-base and the coord's folder, where agy-rpc is and where it finds the pinned t3code and agy.
async fn agy_places(shared: &Shared) -> (std::path::PathBuf, std::path::PathBuf) {
    let core = shared.lock().await;
    (std::path::PathBuf::from(&core.base), std::env::var_os("COORD").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(&core.coord)))
}

/// One start of an agy account's t3code server at a time, in this coordd: two would both find none running.
static AGY_START: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The t3code server of agy account ID, one t3code signed in ([`accounts::agy::legacy`]), started unless one runs
/// ([`accounts::agy::start_server`], in its own cgroup), and waited for until it is up. At coordd's start, and before
/// each check, sign-out and agent's start on it (the server outlives coordd's restarts, so a running one is taken as it
/// is).
async fn agy_server_up(shared: &Shared, store: &accounts::Store, id: &str) -> Result<(), String> {
    use accounts::{agy, Harness};
    let (base, coord) = agy_places(shared).await;
    let _one = AGY_START.lock().await;
    let (st, i) = (store.clone(), id.to_string());
    tokio::task::spawn_blocking(move || st.layout(Harness::Agy, &i)).await.map_err(|e| e.to_string())??;
    let dir = store.dir(Harness::Agy, id);
    let pid = agy::start_server(&base, &coord, &dir, &agy::server_group(id))?;
    agy::server_up(&dir, pid).await
}

/// The t3code server of each agy account t3code signed in at coordd's start, one after another, in a task of its own:
/// a server that runs already is taken as it is. One that does not come up is said in the journal; its next check
/// tries again.
async fn agy_servers(shared: Shared) {
    use accounts::{agy, Harness, Kind};
    let store = shared.lock().await.accounts.clone();
    let st = store.clone();
    let ids: Vec<String> = tokio::task::spawn_blocking(move || {
        st.list(Harness::Agy).into_iter().filter(|e| e.account.kind == Kind::Signin && e.state.leaving.is_none() && agy::legacy(&st.dir(Harness::Agy, &e.id))).map(|e| e.id).collect()
    })
    .await
    .unwrap_or_default();
    for id in ids {
        if let Err(why) = agy_server_up(&shared, &store, &id).await {
            eprintln!("coordd: account {id}: its t3code server is not up: {why}");
        }
    }
}

/// `AccountSignin`, `AccountCode`, `AccountCancel` and `AccountCheck` of agy account ID: agy's own Google sign-in into
/// the account's profile ([`accounts::agy::SignIn`], in the account's own cgroup): its first answer is Google's address
/// as agy gives it, then the code Google shows him goes to agy, once. An account t3code signed in is signed in so only
/// once none of its agents runs (they run in its t3code server, which is then ended): after it, the account runs on
/// agy's own client. A key account has no sign-in: only its check.
async fn agy_signin(shared: &Shared, who: &Who, store: &accounts::Store, id: &str, req: Request) -> Reply {
    use accounts::{agy, Harness, Kind};
    let running = shared.lock().await.agy_signins.get(id).filter(|r| r.ended().is_none()).cloned();
    let Some(entry) = store.get(Harness::Agy, id) else { return err(format!("coord account: no account {id}")) };
    if entry.account.kind == Kind::Key && !matches!(req, Request::AccountCheck { .. }) {
        return err(format!("coord account: account {id} runs on a Gemini API key, not on a sign-in (its key field)"));
    }
    match req {
        Request::AccountSignin { .. } => {
            if entry.state.leaving.is_some() {
                return err(format!("coord account signin: account {id} is being removed"));
            }
            let run = match running {
                Some(r) => r,
                None => {
                    let (base, coord) = agy_places(shared).await;
                    let dir = store.dir(Harness::Agy, id);
                    if agy::legacy(&dir) {
                        if let Some(why) = store.in_use(Harness::Agy, id) {
                            return err(format!("coord account signin: {why}, through its t3code server: stop them before it is signed in again"));
                        }
                        if let Some(pid) = agy::server_pid(&dir) {
                            end_server(pid).await;
                        }
                    }
                    let (st, i) = (store.clone(), id.to_string());
                    match tokio::task::spawn_blocking(move || st.layout(Harness::Agy, &i)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(why)) => return err(format!("coord account signin: {why}")),
                        Err(e) => return err(format!("coord account signin: {e}")),
                    }
                    let r = match agy::SignIn::start(&base, &coord, &dir, &agy::probe_group(id)) {
                        Ok(r) => Arc::new(r),
                        Err(why) => return err(format!("coord account signin: {why}")),
                    };
                    let mut core = shared.lock().await;
                    match core.agy_signins.get(id).filter(|x| x.ended().is_none()).cloned() {
                        Some(first) => {
                            drop(core);
                            r.cancel().await;
                            first
                        }
                        None => {
                            core.agy_signins.insert(id.to_string(), r.clone());
                            drop(core);
                            tokio::spawn(agy_signin_ended(shared.clone(), id.to_string(), r.clone()));
                            r
                        }
                    }
                }
            };
            match run.url().await {
                Ok(url) => ok(format!("{url}\nOpen it, sign in with Google, then paste the code Google shows.")),
                Err(why) => err(format!("coord account signin: {why}")),
            }
        }
        Request::AccountCode { code, .. } => {
            let Some(run) = running else { return err(format!("coord account code: no sign-in runs for {id}")) };
            if let Err(why) = run.paste(&code).await {
                return err(format!("coord account code: {why}"));
            }
            match run.wait().await {
                accounts::signin::Ended::Ok => agy_check(shared, store, id).await,
                accounts::signin::Ended::Failed(why) => err(format!("coord account code: the sign-in failed: {why}")),
            }
        }
        Request::AccountCancel { .. } => match running {
            Some(r) => {
                r.cancel().await;
                ok(format!("account {id}: its sign-in is ended"))
            }
            None => ok(format!("account {id}: no sign-in runs")),
        },
        _ => {
            let r = agy_check(shared, store, id).await;
            match running.filter(|_| matches!(who, Who::Owner)) {
                Some(run) if r.ok => ok(format!("{}\na sign-in runs for it; its words so far:\n{}", r.out, run.printed())),
                _ => r,
            }
        }
    }
}

/// What agy account ID is, asked and recorded, each ask in the account's own cgroup ([`accounts::agy::probe_group`]):
/// of a key account whether its key file is set and which models agy offers on the key; of a sign-in account whether
/// agy takes its sign-in and which models agy offers on it (nothing asked while its sign-in runs); of an account t3code
/// signed in what t3code says on its server (signed in, its models, its limits as t3code read them;
/// `accounts::agy::check`). Whoever waits for an account looks again (`Core::account_checked`).
async fn agy_check(shared: &Shared, store: &accounts::Store, id: &str) -> Reply {
    use accounts::{agy, mover, Harness, Kind};
    let (base, coord) = agy_places(shared).await;
    let dir = store.dir(Harness::Agy, id);
    let group = agy::probe_group(id);
    let now = mover::now_s();
    let (st, i) = (store.clone(), id.to_string());
    let key = store.kind(Harness::Agy, id) == Some(Kind::Key);
    let answer = if key || !agy::legacy(&dir) {
        if !key && shared.lock().await.agy_signins.get(id).is_some_and(|r| r.ended().is_none()) {
            return ok(format!("account {id}: its sign-in runs; it is checked when the sign-in ends"));
        }
        let way = match agy::way_in(store, id) {
            Ok(way) => way,
            Err(why) => return err(format!("coord account check: {why}")),
        };
        let key_file = key.then(|| store.key_file(Harness::Agy, id));
        let models = if way.is_some() { agy::models(&base, &coord, &dir, &group, key_file.as_deref()).await } else { Ok(vec![]) };
        let (what, none) = if key { ("key", "no key") } else { ("Google sign-in (agy's own)", "not signed in") };
        let said = match (&models, way) {
            (_, None) => none.to_string(),
            (Ok(m), Some(_)) => format!("{what}; agy offers {} models on it", m.len()),
            (Err(why), Some(_)) => format!("{what}; agy's models were not read: {why}"),
        };
        match tokio::task::spawn_blocking(move || agy::record_own(&st, &i, way, &models, now)).await {
            Ok(Ok(_)) => ok(format!("account {id}: {said}")),
            Ok(Err(why)) => err(format!("coord account check: {why}")),
            Err(e) => err(format!("coord account check: {e}")),
        }
    } else {
        if let Err(why) = agy_server_up(shared, store, id).await {
            return err(format!("coord account check: {why}"));
        }
        let c = match agy::check(&base, &coord, &dir, &group).await {
            Ok(c) => c,
            Err(why) => return err(format!("coord account check: {why}")),
        };
        let runtime = if c.installed { format!("agy {}", c.version.clone().unwrap_or_default()) } else { "t3code has not installed its agy yet".into() };
        // what t3code read of its limits on this check, or why none: its own words (an expired sign-in, a failed read)
        let limits = match c.limits.as_ref().map(|l| (l["windows"].as_array().map_or(0, Vec::len), l["unavailable"]["message"].as_str())) {
            Some((0, Some(why))) => format!("; no limits read: t3code says {why}"),
            Some((0, None)) | None => "; no limits read".to_string(),
            Some((n, _)) => format!("; {n} limit windows read"),
        };
        // a refresh t3code refused: what it says is what it had, not read again, and why (docket-core N-1446 p2503)
        let refresh = c.refresh_failed.as_ref().map(|why| format!("; t3code did not read it again: {}", why.split_whitespace().collect::<Vec<_>>().join(" "))).unwrap_or_default();
        let said = if c.signed_in() {
            format!("signed in (t3code: {}); {} models of t3code's; {runtime}{limits}{refresh}; signed in again, it runs on agy's own client", c.auth.label.clone().or(c.auth.kind.clone()).unwrap_or_default(), c.models.len())
        } else {
            format!("not signed in (t3code says {}); {runtime}{refresh}", c.auth.status.clone().unwrap_or_else(|| "nothing".into()))
        };
        match tokio::task::spawn_blocking(move || agy::record(&st, &i, &c, now)).await {
            Ok(Ok(_)) => ok(format!("account {id}: {said}")),
            Ok(Err(why)) => err(format!("coord account check: {why}")),
            Err(e) => err(format!("coord account check: {e}")),
        }
    };
    shared.lock().await.account_checked(id);
    answer
}

/// Each signed-in agy account checked once as coordd starts ([`agy_check`]), so its models are this coordd's reading,
/// not what a check of an older one kept.
fn check_agy_at_start(shared: Shared) {
    tokio::spawn(async move {
        let store = shared.lock().await.accounts.clone();
        for e in store.list(accounts::Harness::Agy).into_iter().filter(|e| e.state.signed_in) {
            let r = agy_check(&shared, &store, &e.id).await;
            eprintln!("coordd: account {} checked at start: {}", e.id, r.out);
        }
    });
}

/// agy's sign-in RUN of agy account ID has ended: what agy now says is recorded, and the handle let go unless a newer
/// run took its place. Its cgroup stays for the account's checks.
async fn agy_signin_ended(shared: Shared, id: String, run: Arc<accounts::agy::SignIn>) {
    let ended = run.wait().await;
    if ended == accounts::signin::Ended::Ok {
        signed_in_anew(&shared, accounts::Harness::Agy, &id).await;
    }
    let store = shared.lock().await.accounts.clone();
    let r = agy_check(&shared, &store, &id).await;
    eprintln!("coordd: account {id}: its sign-in ended ({}); {}", match ended { accounts::signin::Ended::Ok => "done".to_string(), accounts::signin::Ended::Failed(why) => why }, r.out);
    let mut core = shared.lock().await;
    if core.agy_signins.get(&id).is_some_and(|x| Arc::ptr_eq(x, &run)) {
        core.agy_signins.remove(&id);
    }
}

/// End the t3code server PID of an agy account t3code signed in, and wait until it has ended.
async fn end_server(pid: i32) {
    // SAFETY: SIGTERM to the account's own t3code server, then kill(2) with 0 to ask whether it is still there
    unsafe { libc::kill(pid, libc::SIGTERM) };
    while unsafe { libc::kill(pid, 0) } == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// The sign-out of agy account whose folder was set aside at ASIDE (KIND its kind): a key account has nothing to sign
/// out of (its key file is set aside with the folder). A sign-in account is signed out by agy itself of its profile;
/// one t3code signed in once by t3code too: its server, which ran from the folder before it was moved, is ended, a
/// server started for the sign-out in the set-aside folder, and ended after.
async fn agy_sign_out(shared: &Shared, aside: &std::path::Path, kind: accounts::Kind) -> Result<(), String> {
    use accounts::agy;
    if kind == accounts::Kind::Key {
        return Ok(());
    }
    if let Some(pid) = agy::server_pid(aside) {
        end_server(pid).await;
    }
    let (base, coord) = agy_places(shared).await;
    let name = aside.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (group, probe) = (agy::server_group(&name), agy::probe_group(&name));
    let server = if agy::home(aside).join(".t3").is_dir() {
        let _one = AGY_START.lock().await;
        let pid = agy::start_server(&base, &coord, aside, &group)?;
        agy::server_up(aside, pid).await?;
        Some(pid)
    } else {
        None
    };
    let out = agy::logout(&base, &coord, aside, &probe).await;
    if let Some(pid) = server {
        end_server(pid).await;
    }
    for g in [group, probe] {
        let _ = std::fs::remove_dir(cgroup::root().join(g));
    }
    out
}

/// The telemetry receiver of the agents that run on an account (accounts/telemetry.rs), started unless it runs: with
/// the first such agent, or at coordd's start when there is one. The sessions that run on an account are named to it
/// (`Core::account_session`). Without it the usage rows come without durations, and the log says why.
async fn telemetry_up(core: &mut Core) {
    if core.telemetry_port.is_some() {
        return;
    }
    let names: Vec<String> = core.agents.values().filter(|a| a.native).map(|a| a.name.clone()).collect();
    for n in names {
        core.account_session(&n);
    }
    let sessions = core.account_sessions.clone();
    let who: accounts::telemetry::Who = Arc::new(move |s: &str| sessions.lock().ok()?.get(s).cloned());
    match accounts::telemetry::start(core.coord.clone(), who).await {
        Ok(port) => core.telemetry_port = Some(port),
        Err(e) => eprintln!("coordd: accounts: no telemetry receiver ({e}): usage rows come without durations"),
    }
}

/// `coord duties` (consult-3, N-581.p19; paper-research N-581.p22): the facts and policy files taken under the lock
/// (`Core::duties_ask`), RUN (eflint-rs, `DutiesJob::run`) on a blocking thread with the lock let go, so every other
/// request is answered meanwhile, and its answer kept under the lock again for the next ask on the same facts and files.
async fn duties_off_lock<F>(shared: &Shared, w: &Who, holder: Option<&str>, run: F) -> Reply
where
    F: FnOnce(crate::core::DutiesJob) -> (crate::core::DutiesJob, Result<Vec<crate::policy::interp::Duty>, String>) + Send + 'static,
{
    let ask = shared.lock().await.duties_ask(w, holder);
    let job = match ask {
        crate::core::DutiesAsk::Done(r) => return r,
        crate::core::DutiesAsk::Run(job) => job,
    };
    let (job, got) = match tokio::task::spawn_blocking(move || run(job)).await {
        Ok(x) => x,
        Err(e) => return err(format!("coord duties: {e}")),
    };
    match got {
        Ok(list) => {
            let r = crate::core::duties_reply(job.holder.as_deref(), &list);
            shared.lock().await.duties_keep(job, list);
            r
        }
        Err(e) => err(format!("coord duties: the norms could not be asked: {e}")),
    }
}

async fn handle(shared: &Shared, caller: &Caller, pid: i32, req: Request) -> Reply {
    // today's item verbs are ticket requests (E2): an item open, or an answer to its close; start stays
    let (req, item_open) = match req {
        Request::Item { action, arg, why } => match items::item_request(&action, arg.clone(), why) {
            Ok(r) => (r, (action == "open").then(|| arg.trim().to_string())),
            Err(e) => return err(e),
        },
        r => (r, None),
    };
    // his comments as data: worded into the post's text before anything reads it, kept on the post once it is made
    let (req, comments) = worded(req);
    if let Request::Run { name } = &req {
        let r = shared.lock().await.run(caller, name, pid);
        return match r {
            Ok(()) => {
                watch_exit(shared.clone(), name.clone(), pid);
                ok(String::new())
            }
            Err(e) => err(e),
        };
    }
    if let Request::Hook { event, input } = &req {
        let Caller::Agent(name) = caller else { return ok(String::new()) }; // not an agent's session: nothing to record
        // a read outside its folders: let through, refused, or held for the owner's answer on its card (read_ask.rs)
        if event == "permission" {
            return ok(read_permission(shared, name, input).await);
        }
        // a Claude Code agent's hook on a tool that decides nothing: the follower woken, no lock taken (hook_quick.rs)
        if hook_quick::answered(name, event, input) {
            return ok(String::new());
        }
        // a boxed agent's turn ended: its layer's commits brought into the project's repository (an answer naming a
        // commit is taken after this)
        if event == "stop" {
            let coord = shared.lock().await.places().coord.clone();
            if sandbox::known_uid(&coord, name).is_some() {
                for e in tokio::task::spawn_blocking(move || layer::sync_open(&coord)).await.unwrap_or_default() {
                    eprintln!("coordd: layers: {e}");
                }
            }
        }
        let next = shared.lock().await.hook(name, event, input);
        return match next {
            HookNext::Done(out) => ok(out),
            HookNext::CodexStop => {
                let (s, n) = (shared.clone(), name.clone());
                tokio::spawn(async move {
                    // wait for the Codex turn to end before held mail goes in as the next one
                    let rpc = {
                        let core = s.lock().await;
                        let a = &core.agents[&n];
                        let mut cmd = tokio::process::Command::new(core::rpc_bin(&core.base, &a.kind));
                        cmd.arg("wait-idle").arg(a.session.get("session_id").and_then(|v| v.as_str()).unwrap_or(""));
                        cmd.env("CODEX_SOCK", a.session.get("socket").and_then(|v| v.as_str()).unwrap_or(""));
                        cmd
                    };
                    let mut rpc = rpc;
                    let _ = rpc.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().await;
                    // the Codex turn has ended: its transcript holds the whole turn
                    let ended = s.lock().await.end_turn(&n);
                    if let Some((turn, bg)) = ended {
                        turn_waits(s, n, turn, bg).await;
                    }
                });
                ok(String::new())
            }
        };
    }
    if let Request::Pty = req {
        // a terminal holder (coord-pty) leaves its caller's cgroup; what it starts is unclaimed until `coord run`
        if *caller == Caller::Project || pid <= 0 {
            return err("coordd: only a terminal holder of a known process asks for the cgroup pty".into());
        }
        return match cgroup::place(pid, "pty") {
            Ok(()) => ok(String::new()),
            Err(e) => err(format!("coordd: cgroup pty: {e}")),
        };
    }
    if let Request::Busy { name, busy, channels_off, no_sign_in } = &req {
        // a terminal holder's (or anyone's but a project source's): a Claude Code agent's state is what its screen shows,
        // and whether Claude Code said there that it takes no channel message
        if *caller == Caller::Project {
            return err("coordd: a project source does not report screens".into());
        }
        let mut core = shared.lock().await;
        core.channels_off(name, channels_off.as_deref());
        core.no_sign_in(name, no_sign_in.as_deref());
        core.busy(name, *busy);
        return ok(String::new());
    }
    if let Request::Popup { name, title, screen } = &req {
        // a terminal holder's report (or anyone's but a project source's): told to the owner once per title
        if *caller == Caller::Project {
            return err("coordd: a project source does not report dialogs".into());
        }
        shared.lock().await.popup(name, title, screen);
        return ok(String::new());
    }
    if let Request::ReadBrain { note } = &req {
        return read_brain(shared, caller, note).await;
    }
    // a quote reply (paper-research N-101 p474; the owner, N-234.p5): the owner's post on the ticket he quoted, under
    // the post quoted, open or closed; typed to nobody, it is to that post's author. No `owner_message`, no intake
    let (req, quoted) = match req {
        Request::Say { follows: Some(_), under: Some(_), .. } => return err("coordd: a subthread (under) and a quote reply (follows) are two things: give one".into()),
        Request::Say { to, text, follows: Some(f), wait, typed, split_from, .. } => {
            // a post or a ticket, as the star reads one (`post_or_ticket`)
            let Some((t, n)) = docket_machine::post_or_ticket(&f).map(|(t, n)| (t.to_string(), n)) else { return err(format!("coordd: {f} is no post")) };
            let to = match to {
                Some(a) => Some(a),
                None => shared.lock().await.ticket_quoted_author(&t, n),
            };
            (Request::Post { kind: None, t, text, to, answers: n, wait, also: Vec::new(), typed, split_from, on: Vec::new(), comments: Vec::new() }, true)
        }
        // an agent's report (pull.rs): its answer to what its pulls handed, a post asked of the policy as any is
        Request::ReportWork { summary, refs, item } => {
            let Some(w) = who(caller) else { return err("coord report-work runs inside an agent session".into()) };
            match shared.lock().await.report_work(&w, &summary, &refs, item.as_deref()) {
                Ok(post) => (post, false),
                Err(e) => return err(e),
            }
        }
        r => (r, false),
    };
    let mut said = ticket::Said::default();
    // `coord end T [T...]` (N-447 p2): policy's `may edit` for each, off the lock; those refused are said by name and
    // the others end
    let (mut ends, mut end_refused): (Vec<(String, ticket::Said)>, Vec<(String, String)>) = (Vec::new(), Vec::new());
    if let (Request::EndThread { ts, .. }, Some(w)) = (&req, who(caller)) {
        for t in ts {
            let ask = shared.lock().await.ticket_may_edit(&w, t);
            match ask {
                None => ends.push((t.clone(), ticket::Said::default())),
                Some(ask) => match tokio::task::spawn_blocking(move || ticket::may(ask)).await {
                    Ok(Ok(s)) => ends.push((t.clone(), s)),
                    Ok(Err(why)) => end_refused.push((t.clone(), why)),
                    Err(e) => return err(format!("coordd: {e}")),
                },
            }
        }
    }
    if let (Request::Cancel { t, .. } | Request::Withdraw { t, .. } | Request::Repair { t, .. } | Request::Rekind { t, .. } | Request::Edit { t, .. } | Request::Answer { t, .. }, Some(w)) = (&req, who(caller)) {
        // a cancel is an edit: policy's `may edit` on the ticket, solved off the lock (the owner is not bound by may);
        // so is answering a hold one does not hold (`coord item close`); a step's answer asks nothing here
        let ask = match &req {
            Request::Cancel { .. } | Request::Withdraw { .. } | Request::Repair { .. } | Request::Rekind { .. } | Request::Edit { .. } => shared.lock().await.ticket_may_edit(&w, t),
            _ => shared.lock().await.ticket_may_answer(&w, t),
        };
        if let Some(ask) = ask {
            match tokio::task::spawn_blocking(move || ticket::may(ask)).await {
                Ok(Ok(s)) => said = s,
                Ok(Err(why)) => return err(why),
                Err(e) => return err(format!("coordd: {e}")),
            }
        }
    }
    if let (Request::Open { .. } | Request::Post { .. } | Request::Msg { .. } | Request::Ask { .. }, Some(w)) = (&req, who(caller)) {
        // notify is asked here too, for the owner's posts as well (the owner is not bound by may); `coord msg` is a post
        // on the ticket it replies to, else a note opened, and `coord ask` a question opened (S5)
        // a ticket's `may`: its facts under the lock, the norms solved off it (plan/core.md §7)
        let ask = shared.lock().await.ticket_may(&w, &req);
        if let Some(ask) = ask {
            match tokio::task::spawn_blocking(move || ticket::may(ask)).await {
                Ok(Ok(s)) => said = s,
                Ok(Err(why)) => return err(why),
                Err(e) => return err(format!("coordd: {e}")),
            }
        }
    }
    // one message on several threads (N-725): each further thread's post asked of the policy as any post is, off the
    // lock; those refused are said by name and the others posted
    let (mut on_said, mut on_refused): (Vec<(String, ticket::Said)>, Vec<(String, String)>) = (Vec::new(), Vec::new());
    if let (Request::Post { t, text, to, wait, on, .. }, Some(w)) = (&req, who(caller)) {
        let mut seen = std::collections::HashSet::from([t.clone()]);
        for o in on.iter().filter(|o| seen.insert(o.to_string())) {
            let one = Request::Post { kind: None, t: o.clone(), text: text.clone(), to: to.clone(), answers: None, wait: *wait, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
            let ask = shared.lock().await.ticket_may(&w, &one);
            match ask {
                None => on_said.push((o.clone(), ticket::Said::default())),
                Some(ask) => match tokio::task::spawn_blocking(move || ticket::may(ask)).await {
                    Ok(Ok(s)) => on_said.push((o.clone(), s)),
                    Ok(Err(why)) => on_refused.push((o.clone(), why)),
                    Err(e) => return err(format!("coordd: {e}")),
                },
            }
        }
    }
    if let Request::Older { id, before } = &req {
        // runs a source (seconds): off the lock
        if who(caller).is_none() {
            return err("coordd: a terminal holder's program or a project source cannot do that".into());
        }
        let Some(project) = shared.lock().await.project.clone() else { return err("coord older: no project file".into()) };
        return match project.older(id, before).await {
            Ok(out) => ok(out),
            Err(e) => err(e),
        };
    }
    let Some(w) = who(caller) else { return err("coordd: a terminal holder's program or a project source cannot do that".into()) };
    // an agent's own check (runner.rs): the runner's command runs off the lock
    // an agent made from an agent's turn (make.rs): the policy and roster off the lock
    if let Request::Make { new, from, model, text, fork } = req {
        return make::make(shared, &w, make::Asked { new, from, model, text, fork }).await;
    }
    // the owner's roster add or restart in a boxed project: started as coordd starts any of its agents (launch)
    if let Request::Start { name, model, prompt, dir, force } = req {
        return start(shared, &w, name, model, prompt, dir, force).await;
    }
    // what the app read on the server's disk itself (N-412 ask 13, p129): the owner's, off the lock
    if matches!(req, Request::Subject { .. } | Request::Harness | Request::HarnessLatest | Request::HarnessSet { .. } | Request::Machines) {
        return server_read(shared, &w, req).await;
    }
    // the owner's typing cleaned by one fresh call (clean.rs, protocol 7): off the lock, nothing written
    if let Request::CleanText { text, keep } = req {
        return clean_text(shared, &w, text, keep).await;
    }
    // his message's points proposed by one fresh call (split.rs): off the lock, nothing written
    if let Request::Split { text } = req {
        return split_text(shared, &w, text).await;
    }
    // the owner's end of a spin-off (his walk item 19): roster run here, off the lock, as for his Fork
    if let Request::SpinEnd { name, how } = req {
        return spin_end(shared, &w, name, how).await;
    }
    // the owner's New agent, Fork and Rename from the app (protocol 1): roster run here, off the lock
    if let Some(r) = OwnerRoster::of(&req) {
        return owner_roster(shared, &w, r).await;
    }
    // the raw pane's lines (N-802): where the log is, taken under the lock; the log read off it
    if let Request::RawPage { name, before, after, span } = req {
        return raw_page(shared, &w, name, before, after, span).await;
    }
    // a page of an agent's session (the app's transcript view): which session, taken under the lock; the page read
    // off it (paper-research N-1446 p2313)
    if let Request::Transcript { name, before, after, session } = req {
        return transcript_page(shared, &w, name, before, after, session).await;
    }
    // the owner's files (protocol 4): read and written here, off the lock, for an app that cannot reach the disk
    if let Some(r) = owner_files(shared, &w, &req).await {
        return r;
    }
    // his Add a repository on the project's Home: the file written off the lock, then read again under it
    if let Request::AddRepo { name, path } = req {
        return add_repo(shared, &w, name, path).await;
    }
    // his answer on an agent's read card: a folder kept in the project file off the lock, then the call let through
    if let Request::ReadAnswer { name, id, answer } = req {
        return read_answer(shared, &w, name, id, answer).await;
    }
    // his Settings page: read, or one written off the lock and the project read again under it
    if let Request::Settings { scope, key, value } = req {
        return settings(shared, &w, scope, key, value).await;
    }
    if matches!(req, Request::RepoState) {
        return repo_state(shared, &w).await;
    }
    // a press on a file, commit or repository: the code view's server found or started off the lock
    if let Request::CodeView { target, theme, looks } = req {
        return code_view(shared, &w, target, theme, looks).await;
    }
    // his messages held to go later: kept and sent off the lock (schedule.rs)
    if let Request::Schedule { act } = req {
        return schedule::act(shared, &w, act).await;
    }
    if matches!(req, Request::CodeViewLeft) {
        if !matches!(w, Who::Owner) {
            return err("coord codeview: the owner's".into());
        }
        codeview::left(std::path::PathBuf::from(&shared.lock().await.coord));
        return ok(String::new());
    }
    if let Request::Check { mode, refs, flip } = req {
        return runner::check(shared, &w, mode, refs, flip).await;
    }
    // an account's sign-in (accounts/signin.rs): Claude Code's own command, run and waited for off the lock
    if matches!(req, Request::AccountSignin { .. } | Request::AccountCode { .. } | Request::AccountCheck { .. } | Request::AccountCancel { .. }) {
        return account_signin(shared, &w, req).await;
    }
    // what changes the accounts' store: its lock is waited for off the core's lock (accounts/mod.rs `Lock`)
    if matches!(req, Request::AccountFor { .. } | Request::MoveAgent { .. } | Request::AccountTier { .. } | Request::AccountEdit { .. } | Request::AccountClear { .. } | Request::AccountKey { .. } | Request::AccountKeyRemove { .. }) {
        return account_store(shared, &w, req).await;
    }
    // a reset applied in a session of its own: off the lock
    if let Request::AccountReset { id, kind } = req {
        return account_reset(shared, &w, &id, &kind).await;
    }
    // an account's removal: Claude Code's own sign-out, run and waited for off the lock as its sign-in is
    if let Request::AccountRemove { id } = req {
        return account_remove(shared, &w, &id, false).await;
    }
    if let Request::AccountKeep { id } = req {
        return account_remove(shared, &w, &id, true).await;
    }
    if let Request::AccountPutBack { name } = req {
        return account_put_back(shared, &w, &name).await;
    }
    // `coord accounts`: what the core knows copied under the lock, the accounts' files and the month's usage read off it
    if let Request::Accounts { json } = req {
        let listing = shared.lock().await.accounts_listing();
        return tokio::task::spawn_blocking(move || listing.reply(json)).await.unwrap_or_else(|e| err(format!("coord accounts: {e}")));
    }
    // the native usage's cells past the reader's cursor (accounts/cells.rs), read off the lock
    if let Request::UsageCells { after } = req {
        if let Some(e) = accounts::usage::refused(matches!(w, Who::Owner)) {
            return err(e);
        }
        let coord = shared.lock().await.coord.clone();
        return tokio::task::spawn_blocking(move || ok(serde_json::to_string(&accounts::cells::served(&coord, after)).unwrap_or_default())).await.unwrap_or_else(|e| err(format!("coordd: usage: {e}")));
    }
    // `coord duties`: the facts and policy files taken under the lock, eflint-rs run off it, its answer kept for the
    // same facts and files (consult-3, N-581.p19)
    if let Request::Duties { holder } = &req {
        return duties_off_lock(shared, &w, holder.as_deref(), |job| {
            let got = job.run();
            (job, got)
        })
        .await;
    }
    // the owner's draft read as a reload reads it (N-1396 p301; consult-3 p306, 3): where from under the lock, the files
    // read and the policy compiled off it, as `read_models` solves, then checked and asked of the tickets under it
    if let Request::Reach { path, text } = &req {
        let (places, first) = match shared.lock().await.ticket_reach_from(&w, path, text) {
            Ok(f) => f,
            Err(r) => return r,
        };
        return match tokio::task::spawn_blocking(move || ticket::Tickets::read_defs(&places, first)).await {
            Ok(read) => shared.lock().await.ticket_reach(read),
            Err(e) => err(format!("coordd: {e}")),
        };
    }
    // the owner's dry run of a prompt (H2, his N-216.p1): who may, the draft's place and the step that fills it under
    // the lock; the calls, renderings and the step's texts read off it; nothing sent, recorded or counted
    if let Request::DryRun { call, on, path, text } = &req {
        let dry = match shared.lock().await.ticket_dry_from(&w, call, on.as_deref(), path.as_deref(), text) {
            Ok(d) => d,
            Err(r) => return r,
        };
        return match tokio::task::spawn_blocking(move || render::dry_run(dry)).await {
            Ok(Ok(a)) => ok(serde_json::to_string(&a).unwrap_or_default()),
            Ok(Err(e)) => err(e),
            Err(e) => err(format!("coordd: {e}")),
        };
    }
    // the owner's edit of a system prompt (protocol 51): its refusals under the lock, the turn's end he picked read in
    // the transcript off it (tens to hundreds of MB, docket-core N-1446 p2471), applied under it with them checked again
    if let Request::SystemPrompt { name, text, replace, after } = &req {
        let file = match shared.lock().await.system_prompt_from(&w, name, text, *replace, *after) {
            Ok(f) => f,
            Err(r) => return r,
        };
        let after = *after;
        return match tokio::task::spawn_blocking(move || core::system_cut(file, after)).await {
            Ok(Ok(cut)) => shared.lock().await.system_prompt(&w, name, text, *replace, cut),
            Ok(Err(r)) => r,
            Err(e) => err(format!("coordd: {e}")),
        };
    }
    // `coord find`: the posts gathered under the lock, read and searched off it (their texts are in session files)
    if let Request::Find { q, starred, wide } = &req {
        let found = shared.lock().await.ticket_find(q, *starred, *wide);
        return match found {
            Ok(r) => r,
            Err(f) => tokio::task::spawn_blocking(move || ticket::find_words(f)).await.unwrap_or_else(|e| err(format!("coordd: {e}"))),
        };
    }
    // the record's graph, the part an ask names (proto-window N-1446 p1383): the slice and its labels, all of them what
    // is kept, git's side read on the side's clock (`read_sides`, paper-research N-1446 p1476), so no ask waits for git.
    // Its posts' words are read, and the answer written, off the lock (p2432)
    if let Request::Graph { ask } = &req {
        let (mut out, words) = shared.lock().await.graph_slice_held(ask);
        return tokio::task::spawn_blocking(move || {
            words.fill(&mut out.labels);
            serde_json::to_string(&out).unwrap_or_default()
        })
        .await
        .map_or_else(|e| err(format!("coordd: {e}")), ok);
    }
    // labels: the tickets', steps', posts' and agents' under the lock, the posts' words and the commits' subjects from
    // git off it
    if let Request::Label { ids } = &req {
        let (mut out, words, commits, repos) = shared.lock().await.labels_held(ids);
        return tokio::task::spawn_blocking(move || {
            words.fill(&mut out);
            label::fill_commits(&mut out, &label::subjects(&commits, &repos));
            out.iter().map(|l| serde_json::to_string(l).unwrap_or_default()).collect::<Vec<_>>().join("\n")
        })
        .await
        .map_or_else(|e| err(format!("coordd: labels: {e}")), ok);
    }
    // how a ticket's answers were read: its number under the lock, the files and the log read off it
    if let Request::Forming { t } = &req {
        let at = shared.lock().await.ticket_forming(t);
        return match at {
            Ok((n, coord)) => tokio::task::spawn_blocking(move || ticket::forming_reads(&coord, n)).await.unwrap_or_else(|e| err(format!("coordd: {e}"))),
            Err(r) => r,
        };
    }
    // the record as written about a ticket (the window's Record layer): its runs under the lock, the file read off it
    if let Request::Record { t, at, after, cap } = &req {
        return record_off_lock(shared, t, *at, *after, *cap).await;
    }
    // a node's words on a click (the record's graph): a slice of a session, its giving under the lock, the record's
    // line and the session's bytes read off it
    if let Request::Raw { id } = &req {
        return raw_off_lock(shared, &w, id).await;
    }
    // insight (1), the record's turns (S6): the file read off the lock, as the Record layer's
    if let Request::Insight { which: coord_types::Insight::WorkTalk } = &req {
        return insight_off_lock(shared, &w).await;
    }
    if let Request::Jobs = req {
        return runner::jobs(shared).await;
    }
    // Home's Store, the runner's service asked (a2h-speed N-1446 p2815); its Layers now, the registry, the runner's job
    // files and the layers' own folders read: both off the lock
    if let Request::Store { row, from } = req {
        let first = row.is_none();
        let r = runner::store(row, from).await;
        // the compute cache after the runner's rows, when compute.json names its folder
        if !first || !r.ok {
            return r;
        }
        let line = r.out.clone();
        return tokio::task::spawn_blocking(move || match machine_state::machines::folder().ok().and_then(|f| compute_status::named(&f)) {
            Some(s) => ok(compute_status::on_store(&line, &s)),
            None => ok(line),
        })
        .await
        .unwrap_or(r);
    }
    if let Request::StoreFree { path } = &req {
        return runner::free(&w, path).await;
    }
    // Home's cold storage: the dry run walks folders, so off the lock and off the runtime's threads
    if let Request::Cold { act, path, rule, on } = req {
        let (coord, places) = {
            let core = shared.lock().await;
            (core.coord.clone(), cold::serve::places(&core))
        };
        let owner = matches!(w, Who::Owner);
        return match tokio::task::spawn_blocking(move || cold::serve::serve(&coord, &places, owner, act.as_deref(), path.as_deref(), rule, on)).await {
            Ok(Ok(out)) => ok(out),
            Ok(Err(why)) => err(why),
            Err(e) => err(format!("coord cold: {e}")),
        };
    }
    if let Request::Layers = req {
        return layers_off_lock(shared).await;
    }
    // the standing texts read off the lock: git names the global one's checkout
    if let Request::Standing = req {
        let (dir, base) = {
            let core = shared.lock().await;
            (core.project.as_ref().map(|p| p.dir.clone()), core.base.clone())
        };
        return match tokio::task::spawn_blocking(move || standing::parts(dir.as_deref(), &base)).await {
            Ok(ps) => ok(serde_json::to_string(&ps).unwrap_or_default()),
            Err(e) => err(format!("coordd: {e}")),
        };
    }
    // the system prompt file an agent was given, read off the lock
    if let Request::Given { name } = req {
        let coord = shared.lock().await.coord.clone();
        return match tokio::task::spawn_blocking(move || standing::given(&coord, &name)).await {
            Ok(Ok(g)) => ok(serde_json::to_string(&g).unwrap_or_default()),
            Ok(Err(why)) => err(why),
            Err(e) => err(format!("could not read the file: {e}")),
        };
    }
    if let Request::End { job, why } = req {
        return runner::end(shared, &w, &job, &why).await;
    }
    // a text at its place (a transcript's span): a file read, off the lock and off the runtime's threads. Under it, at
    // 22:48 20 Nov, 1101 of a window's waited 12 s each, every one a connection, until coordd had no descriptor left to
    // open its record log with (consult-3 N-1396 p382, 1). Only at a place a line of the record gives a text at
    // (`ticket::text_places`): coordd runs as him, so a place its caller made up is never read, whatever file it names
    // (paper-research N-1446 p1419, 2); the refusal names the place and none of its words
    if let Request::Text { loc } = req {
        if !shared.1.read().unwrap_or_else(|e| e.into_inner()).holds(&loc) {
            return err(format!("no line of the record gives a text at {}:{} ({} bytes), so it is not read", loc.file, loc.offset, loc.len));
        }
        return tokio::task::spawn_blocking(move || match follow::text_at(&loc.file, loc.offset, loc.len) {
            Some(t) => ok(t),
            None => err(format!("no text at {}:{} any more", loc.file, loc.offset)),
        })
        .await
        .unwrap_or_else(|e| err(format!("coordd: {e}")));
    }
    // the owner's words typed to nobody go to whoever fills lead (design-two-systems §16, 3): policy asked off the lock
    let mut lead = None;
    let ask = {
        let core = shared.lock().await;
        core.needs_lead(&w, &req).then(|| core.lead_ask())
    };
    if let Some((places, facts)) = ask {
        match tokio::task::spawn_blocking(move || ticket::lead_of(&places, facts)).await {
            Ok(l) => lead = l,
            Err(e) => return err(format!("coordd: {e}")),
        }
    }
    // a Codex agent's effort levels are its model's, from Codex's catalog (`codex debug models`, seconds): off the lock
    let mut codex_levels = None;
    if let Request::Effort { name, .. } = &req {
        let model = shared.lock().await.agents.get(name).filter(|a| a.kind == "codex").map(|a| a.model.split(':').next().unwrap_or_default().to_string());
        if let Some(model) = model {
            codex_levels = Some(codex_efforts(&model).await);
        }
    }
    // E (N-101 p406): an agent's answer that leaves uncommitted changes in its layer gets a warning, read off the lock
    let mut uncommitted = String::new();
    if let (Request::Answer { .. }, Caller::Agent(me)) = (&req, caller) {
        let (coord, me) = (shared.lock().await.coord.clone(), me.clone());
        uncommitted = tokio::task::spawn_blocking(move || core::Core::uncommitted_warning(&coord, &me)).await.unwrap_or_default();
    }
    let mut core = shared.lock().await;
    // a record log that cannot be written: what would write the ticket log or the mail log is refused; reads go on
    if writes_record(&req) {
        if let Some(why) = core.log_fault() {
            return err(why);
        }
    }
    // the owner's "when done" (N-101 p1216): the posts his request makes wait for the turn's end of whom they reach
    let wait = matches!(w, Who::Owner) && matches!(&req, Request::Post { wait: true, .. } | Request::Msg { wait: true, .. } | Request::Say { wait: true, .. });
    let before = if wait { core.ticket_owner_posts() } else { Default::default() };
    // a post or a message: its reply carries the post it made, ticket and number (N-1788, `Reply::post`)
    let posts = matches!(&req, Request::Post { .. } | Request::Msg { .. } | Request::Say { .. } | Request::Ask { .. });
    core.tickets.posted = None;
    let reply = match req {
        Request::Msg { to, body, esc, soon: _, wait: _, as_host, re, quotes, comments: _, origin, under, label, kind } => {
            core.ticket_msg_under(&w, as_host, to, &body, esc, re, origin, quotes, said, under, label, kind)
        }
        Request::CommentResolve { t, n, id, resolved } => core.ticket_comment_resolve(&w, &t, n, &id, resolved),
        Request::Ask { body, on } => core.ticket_ask(&w, &body, on, said),
        Request::Drop { id, why } => core.drop_question(&w, &id, why),
        Request::Unfile { id } => core.unfile(&w, &id),
        Request::Release { id } => core.release(&w, &id).await,
        Request::Unsend { id } => core.unsend(&w, &id),
        Request::Esc { to, step } => core.ticket_esc_step(&w, to, step),
        Request::EscAll => core.ticket_esc_all(&w),
        Request::MailHold { on } => core.ticket_mail_hold(&w, on),
        // his switch of an agent's brain: the owner's alone, one `brain_switch` line a press that changes it
        Request::Brain { agent, on } => core.ticket_brain(&w, &agent, on),
        // his sign-off of an ask with his word of worth: the owner's alone, one `signed_off` line a press that changes it
        Request::SignOff { post, worth } => core.ticket_sign_off(&w, &post, &worth),
        Request::Effort { name, level } => core.effort(&w, &name, &level, codex_levels),
        Request::Model { name, model } => core.model(&w, &name, &model),
        Request::Restart { name } => core.restart(&w, &name),
        Request::Mode { name, mode } => core.mode(&w, &name, &mode),
        Request::Compact { name, tokens } => core.compact(&w, &name, tokens),
        Request::Enabled { thing, actor } => core.enabled(&w, thing.as_deref(), actor.as_deref()),
        Request::Duties { .. } => err("coordd: the duties are answered off the lock".into()),
        Request::Questions => core.questions(&w),
        Request::Mail { id } => core.mail_list(&w, id.as_deref()),
        Request::State => core.state_lines(),
        Request::Log { name, last } => core.log(&w, &name, last),
        Request::Item { arg, .. } => core.item_start(&w, &arg),
        Request::Kind { kind } => core.ticket_kind(&kind),
        Request::Kinds => core.ticket_kinds(),
        Request::LayerSync => {
            let coord = core.places().coord.clone();
            drop(core);
            let failed = tokio::task::spawn_blocking(move || layer::sync_open(&coord)).await.unwrap_or_default();
            for e in &failed {
                eprintln!("coordd: layers: {e}");
            }
            return if failed.is_empty() { ok(String::new()) } else { err(failed.join("; ")) };
        }
        Request::Text { .. } => err("coordd: a text is answered off the lock".into()),
        // the owner's message to an agent: a thread, or a one-off by the watcher's intake (design-two-systems §3)
        Request::Say { to, names, text, under, label, thread, typed, split_from, .. } => {
            // his words cleaned (N-487, N-564): the words ticket is checked before anything is sent, and the cleaned
            // line goes on the post of his that the say made with TEXT
            let words = match typed.as_deref().map(|id| core.ticket_cleaned_words(&w, id)).transpose() {
                Ok(v) => v,
                Err(e) => return err(e),
            };
            // a point of his message split by point (N-696): checked before anything is sent, recorded as `cleaned` is
            if let Some(e) = split_from.as_ref().and_then(|f| ticket::split_refused(&w, f)) {
                return err(e);
            }
            match to {
                Some(to) => core.ticket_say_split(&w, to.clone(), vec![to], names, text, under, label, thread, words, split_from),
                None => match lead {
                    Some(l) => core.ticket_say_split(&w, l, Vec::new(), names, text, under, label, thread, words, split_from),
                    None => err("nobody fills lead to take a message typed to nobody: pick an agent".into()),
                },
            }
        }
        Request::NothingToAnswer { t, n } => core.ticket_nothing_to_answer(&w, &t, n),
        Request::WithdrawAsks { t, n, why } => core.ticket_withdraw_asks(&w, &t, n, why),
        Request::TakeBack { t, n } => core.ticket_take_back(&w, &t, n),
        Request::Pass { t, n, to, why } => core.ticket_pass(&w, &t, n, &to, why),
        Request::Writing { agent, on, thread: None } => core.owner_writing(&w, &agent, on, pid),
        Request::Writing { on, thread: Some(t), .. } => core.owner_writing_on(&w, &t, on, pid),
        Request::Star { t, on } => core.ticket_star(&w, &t, on),
        Request::Archive { t, on } => core.ticket_archive(&w, &t, on),
        Request::Tie { t, to } => core.ticket_tie(&w, &t, &to),
        Request::Title { t, label } => core.ticket_title(&w, &t, &label),
        Request::EndThread { how, why, .. } => core.ticket_end_all(&w, std::mem::take(&mut ends), std::mem::take(&mut end_refused), &how, &why),
        Request::Outline => core.ticket_outline(),
        Request::Insight { which } => core.insight(&w, which),
        Request::WindowRelease { release, since } => core.window_release(&w, &release, since.as_deref(), own_release().as_deref()),
        Request::Hold { on } => core.hold_deliveries(&w, on),
        Request::ReadBrain { .. } | Request::ReportWork { .. } => err("coordd: read-brain and report-work are answered before this".into()),
        Request::Notice { to, text } => core.coord_notice(&w, &to, &text),
        Request::Slow { t, on } => core.ticket_slow(&w, &t, on),
        Request::Approve { t, n } => core.ticket_approve(&w, &t, n),
        Request::HaltReject { ticket, halt, reason } => core.ticket_halt_reject(&w, ticket, halt, reason),
        Request::AgentPace { name, mode } => core.set_agent_mode(&w, &name, mode),
        Request::HarnessSwitches { name, safety_check, tools_off, parts_off } => core.harness_switches(&w, &name, safety_check, tools_off, parts_off),
        Request::DeliverUrgent { agent, interrupt } => core.deliver_urgent(&w, &agent, interrupt),
        Request::HaltEdit { t, n, value } => core.ticket_halt_edit(&w, &t, n, value),
        Request::Undo { t } => core.ticket_undo(&w, &t),
        Request::Move { t, n, to } => core.ticket_move(&w, &t, n, to),
        Request::Keep { ts } => core.ticket_keep(&w, ts),
        Request::DropThread { t } => core.ticket_drop(&w, &t),
        Request::Open { kind, under, form, text } => {
            let r = core.ticket_open(&w, kind, under, form, text, said);
            match &item_open {
                Some(title) => core.item_opened(&w, title, r),
                None => r,
            }
        }
        Request::Answer { t, head, text } => {
            let mut r = core.ticket_answer_request(&w, &t, head, text);
            if r.ok {
                r.out.push_str(&uncommitted);
            }
            r
        }
        Request::Cancel { t, why } => core.ticket_cancel(&w, &t, why, said),
        Request::Withdraw { t, program, why } => core.ticket_withdraw(&w, &t, program, why, said),
        Request::Repair { t, program, why } => core.ticket_repair(&w, &t, program, why, said),
        Request::Edit { t, program, why } => core.ticket_edit_program(&w, &t, program, why, said),
        Request::Rekind { t, kind, form, why } => core.ticket_rekind(&w, &t, kind, form, why, said),
        Request::Template { t, call, scope, text, why } => core.ticket_template(&w, &t, call, scope, text, why),
        Request::Reload { commit, why } => core.ticket_reload(&w, commit, why),
        Request::Post { t, text, to, answers, also, typed, split_from, on, kind, .. } => {
            // his words cleaned (N-487): the words ticket is checked before anything is posted
            let words = match typed.as_deref().map(|id| core.ticket_cleaned_words(&w, id)).transpose() {
                Ok(v) => v,
                Err(e) => return err(e),
            };
            if let Some(e) = split_from.as_ref().and_then(|f| ticket::split_refused(&w, f)) {
                return err(e);
            }
            let before = core.ticket_last_post(&t);
            let words_on = (!on.is_empty()).then(|| (text.clone(), to.clone(), lead.clone(), kind.clone()));
            let r = core.ticket_post_typed(&w, &t, text, to, answers, said, lead, quoted, kind);
            let r = if r.ok && !also.is_empty() { core.ticket_answers_also(&w, &t, &also, r) } else { r };
            let r = match words_on {
                Some((text, to, lead, kind)) if r.ok => core.ticket_post_on(&w, &t, before, &text, to, lead, kind, std::mem::take(&mut on_said), std::mem::take(&mut on_refused), r),
                _ => r,
            };
            let r = match words {
                Some(words) if r.ok => core.ticket_cleaned(&w, &t, before, words, r),
                _ => r,
            };
            match split_from {
                Some(from) if r.ok => core.ticket_split_post(&w, &t, before, from, r),
                _ => r,
            }
        }
        Request::Ticket { t } => core.ticket_show(&t),
        Request::Mentions { id } => core.ticket_mentions(&id),
        Request::Find { .. } => err("coordd: find is answered off the lock".into()),
        Request::Reach { .. } => err("coordd: a reach is answered off the lock".into()),
        Request::DryRun { .. } => err("coordd: a dry run is rendered off the lock".into()),
        Request::Label { .. } => err("coordd: labels are answered off the lock".into()),
        Request::Graph { .. } => err("coordd: the graph is answered off the lock".into()),
        Request::Forming { .. } => err("coordd: forming is answered off the lock".into()),
        Request::Record { .. } => err("coordd: the record is read off the lock".into()),
        Request::Raw { .. } => err("coordd: a node's words are read off the lock".into()),
        Request::Tickets { filter } => core.ticket_list(&w, &filter),
        Request::Add { name, kind, model, renderings, from } => match renderings.as_deref().map(core::renderings_name).transpose() {
            Err(why) => err(why),
            Ok(_) => {
                let r = core.add_from(&w, &name, &kind, &model, from.as_deref());
                if let (true, Some(set)) = (r.ok, renderings) {
                    let _ = core.set_renderings(&name, &set);
                }
                r
            }
        },
        Request::Go { to } => core.go(&w, to),
        Request::Rename { name, to } => core.rename(&w, &name, &to),
        Request::Role { name, role } => core.set_role(&w, &name, &role),
        // the owner's accounts (accounts/mover.rs, N-412 ask 12)
        Request::Native { name, on } => core.native(&w, &name, on),
        Request::AccountsAll { on } => core.accounts_all(&w, on),
        Request::AccountBound { name } => core.account_bound(&w, &name),
        Request::GatewayCleared { family } => core.gateway_cleared(&w, &family),
        Request::AccountFor { .. } | Request::MoveAgent { .. } | Request::AccountTier { .. } | Request::AccountEdit { .. } | Request::AccountClear { .. } | Request::AccountKey { .. } | Request::AccountKeyRemove { .. } => {
            err("coordd: a change of the accounts' store is answered off the lock".into())
        }
        Request::Accounts { .. } => err("coordd: the accounts are answered off the lock".into()),
        Request::UsageCells { .. } => err("coordd: the usage is answered off the lock".into()),
        Request::AccountNew { id, label, tier, key, harness, provider } => core.account_new(&w, &id, label.as_deref(), tier, key, harness.as_deref(), provider.as_deref()),
        Request::AccountKeyStore { .. } => err("coordd: an account's key is taken off the lock".into()),
        Request::AccountRemove { .. } | Request::AccountKeep { .. } | Request::AccountPutBack { .. } => err("coordd: an account's removal is answered off the lock".into()),
        Request::AccountReset { .. } => err("coordd: an account's reset is answered off the lock".into()),
        Request::AccountSignin { .. } | Request::AccountCode { .. } | Request::AccountCheck { .. } | Request::AccountCancel { .. } => err("coordd: an account's sign-in is answered off the lock".into()),
        Request::Ended { name, undo } => core.set_ended(&w, &name, undo),
        Request::SystemPrompt { .. } => err("coordd: a system prompt is answered off the lock".into()),
        Request::TeamAsk { text } => core.team_ask(&w, &text).await,
        Request::Act { is, text, more } => core.request_act(&w, &is, &text, &more),
        Request::Check { .. } | Request::Jobs | Request::End { .. } | Request::Make { .. } | Request::Start { .. } | Request::AddAgent { .. } | Request::ForkAgent { .. } | Request::RenameAgent { .. } | Request::SpinEnd { .. } | Request::Subject { .. } | Request::Harness | Request::HarnessLatest | Request::HarnessSet { .. } | Request::Machines
        | Request::CleanText { .. }
        | Request::Split { .. }
        | Request::Roots
        | Request::ListDir { .. }
        | Request::ReadFile { .. }
        | Request::Locate { .. }
        | Request::Sources
        | Request::EditFile { .. }
        | Request::AddRepo { .. }
        | Request::ReadAnswer { .. }
        | Request::Settings { .. }
        | Request::RepoState
        | Request::PdfPage { .. }
        | Request::CodeView { .. }
        | Request::CodeViewLeft
        | Request::Schedule { .. }
        | Request::RawPage { .. }
        | Request::Transcript { .. }
        | Request::Store { .. }
        | Request::StoreFree { .. }
        | Request::Cold { .. }
        | Request::Layers
        | Request::Standing
        | Request::Given { .. } => err("coordd: taken before the lock".into()),
        Request::AskWait { .. } => err("coordd: the ask tool is an agent's".into()),
        Request::TeamGrant { name, granted } => core.team_grant(&w, &name, granted).await,
        Request::Norm { act, args, thread } => core.norm(&act, &args, &thread),
        Request::Models => ok(core.runnable_models().join("\n")),
        Request::ModelList => ok(serde_json::to_string(&core.model_list()).unwrap_or_default()),
        Request::HarnessCapabilities => ok(serde_json::to_string(&coord_types::harness::HarnessCapabilities::all()).unwrap_or_default()),
        Request::Session { name, session } => core.session(&w, &name, &session),
        Request::Image { name, session, offset, n } => core.transcript_image(&w, &name, &session, offset, n),
        // where the proxy's log is; the app reads it on the host (N-40)
        Request::RawSource { name } => match core.proxy_source(&w, &name) {
            Ok(s) => ok(serde_json::to_string(&s).unwrap()),
            Err(e) => err(e),
        },
        Request::Requests { since } => core.requests(&w, since),
        Request::Attach { .. } | Request::Keys { .. } | Request::Resize { .. } | Request::Detach { .. } => {
            err("coordd: terminals are reached through coordc (attach, or the bridge), not coordd".into())
        }
        Request::Lsp { .. } => err("coordd: the shared language servers are reached through the bridge, not coordd".into()),
        Request::Run { .. } | Request::Hook { .. } | Request::Watch { .. } | Request::Pty | Request::Popup { .. } | Request::Busy { .. } | Request::Older { .. } => unreachable!(),
    };
    if wait && reply.ok {
        core.ticket_wait_posts(&before);
    }
    let reply = match comments.is_empty() || !reply.ok {
        true => reply,
        false => match core.ticket_comments(comments) {
            Ok(_) => reply,
            Err(e) => Reply { out: format!("{}; its comments were not kept: {e}", reply.out), ..reply },
        },
    };
    let posted = core.tickets.posted.take();
    if posts && reply.ok && reply.post.is_none() {
        return Reply { post: posted, ..reply };
    }
    reply
}

/// REQ with the comments a post of his carries worded into its text (`docket_machine::comment::render`) unless it
/// begins with them already, as the window sends it, and those comments, taken out of it.
fn worded(req: Request) -> (Request, Vec<docket_machine::Comment>) {
    let render = |c: &[docket_machine::Comment], t: &str| if t.starts_with(&docket_machine::comment::render(c, "")) { t.to_string() } else { docket_machine::comment::render(c, t) };
    match req {
        Request::Msg { body, comments, to, esc, soon, wait, as_host, re, quotes, origin, under, label, kind } if !comments.is_empty() => {
            (Request::Msg { body: render(&comments, &body), comments: Vec::new(), to, esc, soon, wait, as_host, re, quotes, origin, under, label, kind }, comments)
        }
        Request::Post { text, comments, t, to, answers, wait, also, typed, split_from, on, kind } if !comments.is_empty() => {
            (Request::Post { text: render(&comments, &text), comments: Vec::new(), t, to, answers, wait, also, typed, split_from, on, kind }, comments)
        }
        Request::Say { text, comments, to, names, follows, wait, under, label, thread, typed, split_from } if !comments.is_empty() => {
            (Request::Say { text: render(&comments, &text), comments: Vec::new(), to, names, follows, wait, under, label, thread, typed, split_from }, comments)
        }
        r => (r, Vec::new()),
    }
}

/// The op REQ is timed under (`request OP`): its variant's name, so a typed post's alias (`typedpost`,
/// `Request::line`) is timed as `post` (proto-reader N-1481 p61).
fn timed_as(req: &Request) -> String {
    serde_json::to_value(req).ok().and_then(|v| v.get("op")?.as_str().map(String::from)).unwrap_or_default()
}

/// Whether REQ writes the ticket log or the mail log (refused while one cannot be written: `Core::log_fault`).
fn writes_record(req: &Request) -> bool {
    matches!(
        req,
        Request::Msg { .. }
            | Request::Ask { .. }
            | Request::Drop { .. }
            | Request::Unfile { .. }
            | Request::Release { .. }
            | Request::Unsend { .. }
            | Request::Esc { .. }
            | Request::EscAll
            | Request::MailHold { .. }
            | Request::Brain { .. }
            | Request::SignOff { .. }
            | Request::WindowRelease { .. }
            | Request::Item { .. }
            | Request::Say { .. }
            | Request::NothingToAnswer { .. }
            | Request::WithdrawAsks { .. }
            | Request::TakeBack { .. }
            | Request::Pass { .. }
            | Request::Star { .. }
            | Request::CommentResolve { .. }
            | Request::Archive { .. }
            | Request::Tie { .. }
            | Request::Title { .. }
            | Request::EndThread { .. }
            | Request::Notice { .. }
            | Request::Slow { .. }
            | Request::Approve { .. }
            | Request::HaltReject { .. }
            | Request::DeliverUrgent { .. }
            | Request::HaltEdit { .. }
            | Request::Undo { .. }
            | Request::Move { .. }
            | Request::Keep { .. }
            | Request::DropThread { .. }
            | Request::Open { .. }
            | Request::Answer { .. }
            | Request::Cancel { .. }
            | Request::Withdraw { .. }
            | Request::Repair { .. }
            | Request::Edit { .. }
            | Request::Rekind { .. }
            | Request::Template { .. }
            | Request::Reload { .. }
            | Request::Post { .. }
    )
}

/// What agent NAME waits on after its turn TURN ended with background work BG: its state, and the waits.
async fn turn_waits(shared: Shared, name: String, turn: u64, bg: Vec<String>) {
    counts::timed("turn waits (a turn's end)", async { shared.lock().await.turn_end_state(&name, turn, bg).await }).await;
}

/// How long the hand-over goes unrun while nothing it reads changes (`follow`): a sweep then (paper-research N-1446
/// p3381).
const HAND_SWEEP: std::time::Duration = std::time::Duration::from_secs(15);

/// Follow the Claude Code agents' transcripts (`follow.rs`), every second and at once when a hook or new mail pokes:
/// the screen (its holder's `Request::Busy`), else the transcript, decides whether a turn runs; the transcript which tool calls run, a turn end found is taken once
/// (`Core::end_turn`, then `turn_waits`), and held mail is handed to every agent not in a turn (design
/// coordd-core-2026-11-10 §3–§4). Files are read off the lock; only new bytes after the first read.
fn follow(shared: Shared) {
    tokio::spawn(counts::scoped(async move {
        let poke = shared.lock().await.poke.clone();
        let mut followers: HashMap<String, follow::Follower> = HashMap::new();
        let mut last_changes = None;
        // what the hand-over last found nothing new in (`Core::hand_key`), and when it last ran
        let (mut hand_seen, mut hand_ran): (Option<u64>, std::time::Instant) = (None, std::time::Instant::now());
        let mut home_links = homes::Keeper::default();
        loop {
            let round = counts::Round::new("clock follow (each second)");
            let (paths, line): (Vec<(String, String)>, Option<(String, Vec<String>, layer::Spec)>) = {
                let core = shared.lock().await;
                let mut marks = counts::Marks::new();
                // a Codex agent's rollout too, for the deliveries it shows and its turns' final output (its turns and tool
                // calls are its hooks', `Core::hook`)
                let paths = core.agents.values().filter(|a| a.running() && core::harness(&a.kind)).filter_map(|a| Some((a.name.clone(), a.transcript()?))).collect();
                // the agents whose plain tool hooks need no lock (hook_quick.rs): every kind but Codex, as `Core::hook`
                hook_quick::set(poke.clone(), core.agents.values().filter(|a| !core::served(&a.kind)).map(|a| a.name.clone()));
                // a boxed project's line, so that an agent standing in an older line's view is moved (moves.rs)
                let line = core.project.as_ref().filter(|p| p.project.boxed).and_then(|_| core.line_spec().ok()).map(|s| (core.coord.clone(), core.agents.values().filter(|a| a.running()).map(|a| a.name.clone()).collect(), s));
                marks.mark("the paths and the line");
                (paths, line)
            };
            followers.retain(|n, _| paths.iter().any(|(p, _)| p == n));
            let mut fs = std::mem::take(&mut followers);
            let Ok((fs, reads, line_now)) = tokio::task::spawn_blocking(move || {
                // the line's heads read only while a running agent stands in a line's view
                let line_now = line.filter(|(coord, names, _)| names.iter().any(|n| moves::stand(coord, n).is_some_and(|l| layer::is_line(&l)))).and_then(|(_, _, s)| layer::line_now(&s).ok());
                let mut reads = Vec::new();
                for (name, path) in paths {
                    let f = fs.entry(name.clone()).or_insert_with(|| follow::Follower::new(&path));
                    if f.path != path {
                        *f = follow::Follower::new(&path); // a new session, or its file found elsewhere
                    }
                    let r = f.read();
                    let tools = r.tools.then(|| f.tools.values().cloned().collect::<Vec<_>>());
                    let output: Vec<(String, u64, u64)> = f.take_outputs().into_iter().map(|(o, l)| (f.path.clone(), o, l)).collect();
                    let start_end = f.take_start_end().map(|(o, l)| docket_machine::Loc { file: f.path.clone(), offset: o, len: l });
                    let at: Vec<(String, String, u64, u64)> = r.at.into_iter().map(|(id, o, l)| (id, f.path.clone(), o, l)).collect();
                    let bare: Vec<(String, String, u64, u64)> = r.bare.into_iter().map(|(b, o, l)| (b, f.path.clone(), o, l)).collect();
                    let typed: Vec<docket_machine::Loc> = r.typed.iter().map(|(o, l)| docket_machine::Loc { file: f.path.clone(), offset: *o, len: *l }).collect();
                    let span = f.said_span().map(|(o, l)| docket_machine::Loc { file: f.path.clone(), offset: o, len: l });
                    let took = (r.entered.into_iter().map(|e| (e, f.path.clone())).collect::<Vec<_>>(), r.queued, r.compacted, f.queued_now().into_iter().map(|(o, _)| (f.path.clone(), o)).collect::<Vec<_>>(), start_end);
                    // what each turn that ended in this read did, by where its output ends (insight (1))
                    let worked: Vec<(String, u64, docket_machine::Worked)> = f.take_worked().into_iter().map(|(e, w)| (f.path.clone(), e, w)).collect();
                    reads.push((name, r.seen, (f.ended, f.unanswered()), f.running_since.clone(), tools, f.model.clone(), f.compact.take(), output, at, bare, r.cut, typed, f.grew, span, r.begins, r.running_old, took, worked, f.take_answers()));
                }
                (fs, reads, line_now)
            })
            .await
            else {
                return;
            };
            followers = fs;
            let mut ends = Vec::new();
            {
                let mut core = shared.lock().await;
                let mut marks = counts::Marks::new();
                let changes = core.tickets.m.changes();
                // since this point in the round before: what that round wrote counts, and what came between
                counts::round_seen(last_changes.is_some_and(|c| c != changes));
                last_changes = Some(changes);
                core.line_now = line_now;
                // the owner's steps whose time has passed go on by themselves (lapse.rs): the kept set only
                core.lapse_round(chrono::Utc::now());
                marks.mark("lapse_round");
                for (name, seen, (ended, unanswered), since, tools, model, compact, output, at, bare, cut, typed, grew, span, begins, running_old, (entered, queued, compacted, queue, start_end), worked, answers) in reads {
                    if !core.agents.get(&name).is_some_and(|a| a.running()) {
                        continue;
                    }
                    // its replies that came through: the agents held on a rate limit of that model, the same way, go on
                    core.answered(&name, &answers);
                    // kept for each turn's end, taken in its place below (`Core::ticket_turn_ended_queued`)
                    if !worked.is_empty() {
                        core.tickets.worked.entry(name.clone()).or_default().extend(worked);
                    }
                    // a turn that ended while coordd was stopped: its output taken first, when the record still holds it
                    let output: Vec<(String, u64, u64)> = core.start_output(&name, start_end).into_iter().chain(output).collect();
                    // each turn's end in this read in its place among what was seen (`Core::transcript_read`)
                    ends.extend(core.transcript_read_queued(&name, at, bare, typed, &output, cut, &begins, running_old, entered, &queued));
                    // what its session holds queued now, read with what entered: rule 19 lets these wait (0.2 A4)
                    let live = &mut core.agents.get_mut(&name).unwrap().live;
                    live.queue = queue;
                    // a dialog coord-pty told of is gone once the session writes again (rule 19's minute counts again)
                    let dialog_gone = live.dialog.as_ref().is_some_and(|(d, _, _)| grew > *d);
                    if dialog_gone {
                        live.dialog = None;
                        core.changed(&name);
                    }
                    if compacted {
                        core.compaction_over(&name);
                    }
                    marks.mark("transcript_read");
                    // SHOWN: the transcript itself has shown the turn's end (what it said, that it ended, or a cut)
                    let shown = !output.is_empty() || ended || cut.is_some();
                    // a Codex agent: the rest is its hooks' (its turns begin at UserPromptSubmit and end when its
                    // app-server says, `HookNext::CodexStop`); its mail is taken when the app-server takes it
                    if core::served(&core.agents[&name].kind) {
                        continue;
                    }
                    // a pick made in this process only: one from before (a fork's copied history, a window read again
                    // after coordd restarts) does not undo a model set since
                    if let Some((m, _)) = model.filter(|(_, at)| at.get(..19) >= core.agents[&name].live.started.get(..19)) {
                        core.set_model(&name, &m);
                    }
                    // a point set with `/autocompact` in this process, the same way
                    if let Some((set, at)) = compact.filter(|(_, at)| at.get(..19) >= core.agents[&name].live.started.get(..19)) {
                        core.compact_typed(&name, set, &at);
                    }
                    // the screen says whether a turn runs, when its holder tells it (`Core::busy`); else the transcript.
                    // A turn the transcript began before this process started does not run in it: its entry (a prompt,
                    // a background task's notice) was left unanswered when the old process went (10 Nov: MEANING
                    // shown working for hours on a notice from 06:39, restarted 08:46, after coordd restarts forgot
                    // its screen)
                    // the hooks before the screen (design-two-systems §10): an interrupt runs no Stop hook, so its entry,
                    // when the transcript ends there, says the hooks' turn is over
                    let ended = core.turn_ended_now(&name, ended, &since, grew, cut.is_some(), unanswered);
                    marks.mark("model, compact, turn_ended_now");
                    if !seen.is_empty() {
                        core.seen(&name, &seen);
                    }
                    marks.mark("seen");
                    let working = core.agents[&name].live.state == "working";
                    if !ended && !working {
                        core.begin_turn(&name);
                    } else if ended && working {
                        if let Some(e) = core.end_turn(&name) {
                            ends.push((name.clone(), e));
                            // ended by its hooks or screen first: its reply is read before anything is handed
                            core.owe_output(&name);
                            // the turn's end taken with no stop in the transcript and none to come (no Stop hook:
                            // the hooks gone stale, the screen): the text said since its last prompt is its final
                            // output, taken once. After a Stop hook the transcript's own stop record comes, with the
                            // text written up to it: taken then, never an earlier message (consult-3, N-807.p69, 1)
                            if output.is_empty() {
                                core.turn_end_without_stop(&name, span);
                            }
                        }
                    }
                    if shown {
                        core.output_read(&name);
                    }
                    marks.mark("begin_turn, end_turn");
                    match tools {
                        Some(t) => core.set_tools(&name, t),
                        None if !ended && !working => core.changed(&name),
                        None => {}
                    }
                    marks.mark("set_tools, changed (the agent's view)");
                }
                core.not_read();
                marks.mark("not_read");
                // a new effort first (`coord effort`): the restarted session is the one handed what is due
                if let Some(jobs) = core.jobs.clone() {
                    for (name, pid) in core.to_restart() {
                        let _ = jobs.send(Job::Restart { name, pid });
                    }
                }
                // a move to another view before anything is handed (moves.rs)
                core.send_moves();
                marks.mark("to_restart, send_moves");
                // each agent's links into its project, as agents and repositories come and go (homes.rs)
                home_links.round(&core);
                marks.mark("agents' homes");
                // a step's agent that owes a step and runs no session: started again for it, in its layer (N-101 p1778)
                if let Some(jobs) = core.jobs.clone() {
                    for (agent, t, call, layer) in core.to_start_owing() {
                        let Some(view) = core.tickets.m.view(t) else { continue };
                        eprintln!("coordd: tickets: {agent} owes a step of {} and runs no session: started again for it, in {}", view.id, moves::shown(layer.as_deref()));
                        let _ = jobs.send(Job::Start { agent, call, view, layer, from: None, cut: None });
                    }
                }
                marks.mark("to_start_owing");
                // held for a return (`Request::Hold`): nothing more is handed, so a turn stopped stays stopped
                if !core.holding {
                    // run when something it reads changed (a record line, an agent's state, a wait run out: `hand_key`),
                    // not every round whether or not anything did (paper-research N-1446 p3381); with nothing changed,
                    // a sweep every HAND_SWEEP, whose handing anything says an input the key misses (`counts::Hand`)
                    let key = core.hand_key();
                    marks.mark("hand_key");
                    let sweep = hand_ran.elapsed() >= HAND_SWEEP;
                    if hand_seen != Some(key) || sweep {
                        let names = core.to_hand_over();
                        marks.mark("to_hand_over");
                        for name in &names {
                            core.hand_over(name);
                        }
                        marks.mark("hand_over");
                        counts::hand_round(true, hand_seen == Some(key), names.len());
                        hand_ran = std::time::Instant::now();
                        // seen only once a round changes nothing itself: what it handed or raised is looked at again
                        // in the next
                        hand_seen = (core.hand_key() == key).then_some(key);
                        marks.mark("hand_key");
                    } else {
                        counts::hand_round(false, false, 0);
                    }
                    core.hand_clerk();
                    marks.mark("hand_clerk");
                    core.hand_minder();
                    marks.mark("hand_minder");
                    core.hand_coord();
                    marks.mark("hand_coord");
                    core.pulls_missed();
                    core.hand_hosts();
                    marks.mark("hand_hosts");
                    core.hand_far();
                    marks.mark("hand_far");
                }
            }
            for (name, (turn, bg)) in ends {
                tokio::spawn(turn_waits(shared.clone(), name, turn, bg));
            }
            drop(round);
            tokio::select! {
                _ = poke.notified() => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            }
        }
    }));
}

/// The agents' senders: each agent's jobs (`core::Job`) in order on a task of its own, off the core's lock, their
/// outcome told back (`Core::handed`).
fn senders(shared: Shared) -> tokio::sync::mpsc::UnboundedSender<Job> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
    tokio::spawn(async move {
        let mut per: HashMap<String, tokio::sync::mpsc::UnboundedSender<Job>> = HashMap::new();
        while let Some(job) = rx.recv().await {
            let w = per.entry(job.name().to_string()).or_insert_with(|| {
                let (wtx, mut wrx) = tokio::sync::mpsc::unbounded_channel::<Job>();
                let s = shared.clone();
                tokio::spawn(async move {
                    let mut next: Option<Job> = None;
                    while let Some(j) = match next.take() { Some(j) => Some(j), None => wrx.recv().await } {
                        // the waits refreshes queued behind one are that one: each reads the tickets as they are when it
                        // runs, so a burst of ticket changes is refreshed once, not once per change under the lock (N-459)
                        if matches!(j, Job::Waits) {
                            while let Ok(more) = wrx.try_recv() {
                                if !matches!(more, Job::Waits) {
                                    next = Some(more);
                                    break;
                                }
                            }
                        }
                        let kind = j.kind();
                        counts::timed(kind, run_job(&s, j)).await;
                    }
                });
                wtx
            });
            let _ = w.send(job);
        }
    });
    tx
}

/// Copy the text at LOC (in AGENT's transcript) to `$COORD/tickets/texts/AGENT.jsonl` as an entry `follow::text_at`
/// reads, and give its place there.
fn pin(coord: &str, agent: &str, loc: &docket_machine::Loc) -> Result<docket_machine::Loc, String> {
    use std::io::Write as _;
    let text = follow::text_at(&loc.file, loc.offset, loc.len).ok_or("the text is not there")?;
    let dir = format!("{coord}/tickets/texts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = format!("{dir}/{agent}.jsonl");
    let line = format!("{}\n", json!({"type": "pinned", "from": {"file": loc.file, "offset": loc.offset, "len": loc.len}, "message": {"content": text}}));
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&file).map_err(|e| e.to_string())?;
    let offset = f.metadata().map_err(|e| e.to_string())?.len();
    f.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    Ok(docket_machine::Loc { file, offset, len: line.len() as u64 })
}

/// The ticket deliveries TICKETS for agent NAME as text and channel meta (`render::delivery`). A step that does not
/// render is not sent: it fails at once with NoCall (`ticket_unrendered`), so no one waits on an agent never told of
/// it. Another delivery that no template renders still goes, as what it is (a notice, never a step).
/// A served session's message parts, each with its head (render::head): what a Codex, opencode or agy session is
/// sent, and what an agy session's pull answers (`read_brain`).
#[allow(clippy::too_many_arguments)]
async fn served_parts(shared: &Shared, name: &str, tickets: &[(docket_machine::Delivery, docket_machine::TicketView)], replies_on: Option<&str>, again: &[String], into: bool, rest: &[String], digest: &[String]) -> Vec<String> {
    if tickets.is_empty() {
        return Vec::new();
    }
    let places = shared.lock().await.places_for(name);
    let mut out = Vec::new();
    for (content, meta) in rendered_all(shared, name, tickets, replies_on, again, into, true, rest, digest).await {
        let f = |k: &str| meta.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
        // the owner's words to an agent holding only them: who they are from, nothing more (render::delivery);
        // a message joining several carries each part's head itself (render::handover)
        let head = if f("id").contains(' ') {
            Ok(String::new())
        } else if f("id").is_empty() {
            render::head(&places, None, &f("from"), None)
        } else {
            render::head(&places, Some(&f("id")), &f("from"), Some((&f("ticket"), &f("kind"), &f("step"))))
        };
        out.push(match head.unwrap_or_else(|e| {
            eprintln!("coordd: {name}: rendering the head of {}: {e}", f("id"));
            String::new()
        }) {
            h if h.is_empty() => content,
            h => format!("{h}\n{content}"),
        });
    }
    out
}

/// `coord read-brain NOTE` (pull.rs): what is due to the caller, an agy agent, handed in the answer as its pull, and
/// its briefing. The owner's words among what is handed are its new directives, each its thread's id and title.
async fn read_brain(shared: &Shared, caller: &Caller, note: &str) -> Reply {
    let Some(w) = who(caller) else { return err("coord read-brain runs inside an agent session".into()) };
    let (name, job) = {
        let mut core = shared.lock().await;
        let name = match core.may_pull(&w) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        let job = core.hand_job(&name, true);
        (name, job)
    };
    let (mut lines, mut ids, mut directives) = (Vec::new(), Vec::new(), Vec::new());
    if let Some(Job::Codex { ids: handed, text, tickets, replies_on, again, into, rest, digest, .. }) = job {
        if !text.is_empty() {
            lines.push(text);
        }
        directives = tickets.iter().filter(|(d, _)| pull::from_owner(d)).map(|(d, v)| if v.title.is_empty() { d.ticket.clone() } else { format!("{}: {}", d.ticket, v.title) }).collect();
        lines.extend(served_parts(shared, &name, &tickets, replies_on.as_deref(), &again, into, &rest, &digest).await);
        shared.lock().await.handed(&name, &handed, true, "pull");
        ids = handed;
    }
    ok(shared.lock().await.briefing(&name, note, lines, ids, directives).to_string())
}

async fn rendered_all(shared: &Shared, name: &str, tickets: &[(docket_machine::Delivery, docket_machine::TicketView)], replies_on: Option<&str>, again: &[String], into: bool, codex: bool, rest: &[String], digest: &[String]) -> Vec<(String, Value)> {
    // into a turn running: what the delivery program delivers there goes whole (no update, no line: runtime-design
    // §6, D is gone). The turn read again under the lock: one that ended since the hand-over takes it as out of a
    // turn, so nothing starts a turn it was not handed for (paper-research N-807.p37), and the footer's count is made
    // again for what now starts one (`replies_on`, which a hand-over into a turn left out)
    let (into, replies_on) = if into {
        let mut core = shared.lock().await;
        let still = core.ticket_still_in_turn(name, tickets);
        let replies_on = if still { replies_on.map(String::from) } else { core.ticket_replies_on(name, &tickets.iter().map(|(d, _)| d.id.clone()).collect::<Vec<_>>()) };
        (still, replies_on)
    } else {
        (false, replies_on.map(String::from))
    };
    let replies_on = replies_on.as_deref();
    // the warning for an answer given by ending the turn in a layer (moves.rs, N-101 p466, 2): this delivery's first
    // line, read off the lock
    let warning = {
        let (places, me) = (shared.lock().await.places_for(name), name.to_string());
        tokio::task::spawn_blocking(move || moves::turn_warning(&places, &me)).await.ok().flatten()
    };
    // said in every rendering until a turn given them ends (a hand-over the session does not take is sent again,
    // rendered again: after a move it often is); that turn's end takes the marker (`ticket_turn_ended`). The jobs
    // stopped with its session, likewise until its next turn's end (`Core::job_notices`, N-1334)
    let notices = shared.lock().await.job_notices(name);
    let mut out = rendered_each(shared, name, tickets, replies_on, again, warning.as_ref().map(|(said, _)| said.as_str()), &notices, into, codex).await;
    // each message's deliveries that hold his posts its turn must answer or have again, each with those posts' ids, in
    // `his` (an object, so beside the meta: `render::event`): the reply tool's Nothing is no answer to them, and names
    // those posts alone (channel.py; paper-research N-412.p2202, 1; consult-3 p2210)
    if !codex {
        let core = shared.lock().await;
        for (_, meta) in out.iter_mut() {
            let ids: Vec<&str> = meta.get("id").and_then(Value::as_str).unwrap_or_default().split(' ').collect();
            let mut his = serde_json::Map::new();
            for (d, _) in tickets.iter().filter(|(d, _)| ids.contains(&d.id.as_str())) {
                let posts = core.tickets.m.his_to_answer(name, d, into);
                if !posts.is_empty() {
                    his.insert(d.id.clone(), json!(posts));
                }
            }
            if !his.is_empty() {
                meta["his"] = Value::Object(his);
            }
        }
    }
    // what the delivery program's message names at its foot, waiting on (`deliver … listing REST`), what goes with it
    // by first lines (`deliver … digest PULL`, I-1480: a line each thread, `digest_named`), and his posts the agent
    // owes an answer to, left unanswered in two turns and not handed again (`owed_posts`, paper-research N-412.p2197,
    // 1): its last lines
    let (places, named, owed, digest) = {
        let core = shared.lock().await;
        let owed = core.tickets.m.owed_posts(name);
        (core.places_for(name), core.rest_named(rest), core.rest_named(&owed), core.digest_named(name, digest))
    };
    if !rest.is_empty() || !owed.is_empty() || !digest.is_empty() {
        // a template's `if` takes anything but null and false: none more is null, and so is a list with none in it
        let more = Some(rest.len().saturating_sub(10)).filter(|n| *n > 0);
        let some = |s: String| Some(s).filter(|s| !s.is_empty());
        let digest = Some(digest).filter(|d| !d.is_empty());
        match render::template_in(&places, "rest", &json!({"rest": some(named), "more": more, "owed": some(owed), "digest": digest})) {
            Ok(Some(foot)) if !foot.trim().is_empty() => {
                if let Some((text, _)) = out.last_mut() {
                    *text = format!("{}\n\n{}", text.trim_end(), foot.trim());
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("coordd: tickets: rendering the rest named to {name}: {e}"),
        }
    }
    out
}

/// Each delivery rendered, the first message beginning with WARNING when there is one, then NOTICES, each a line
/// (hs-ls, N-1334 p6: in a message joining several, under its head, `handover.tmpl`). Two or more on two or more
/// tickets go as one message (`render::handover`, 0.2 line 211): out of a turn, all of them; into a turn, the owner's
/// posts, an agent's post the delivery program's `arrived` puts in going apart as before. CODEX: the message is for a
/// Codex thread, each part under the head its reader finds the delivery by.
#[allow(clippy::too_many_arguments)]
async fn rendered_each(shared: &Shared, name: &str, tickets: &[(docket_machine::Delivery, docket_machine::TicketView)], replies_on: Option<&str>, again: &[String], warning: Option<&str>, notices: &[String], into: bool, codex: bool) -> Vec<(String, Value)> {
    let (places, held, open_on, mute, notes, untitled, heads) = {
        let core = shared.lock().await;
        let m = &core.tickets.m;
        // the owner's thread with no title, which NAME may title (`Line::Titled`, N-807.p4): a lead's, and the agent he
        // wrote to, its first title taken live (docket-core3, N-807.p12; his "good titles", N-985)
        let untitled: Vec<bool> = tickets.iter().map(|(d, _)| core.ticket_untitled_for(name, d.t)).collect();
        let mute: Vec<Option<Vec<String>>> = tickets.iter().map(|(d, _)| core.ticket_reply_names(d)).collect();
        // the note line (paper-research N-101 p410): a step in another layer than the one NAME stands in says so, and
        // that `coord go` moves it there (moves.rs: a Claude Code or a Codex agent)
        let places = core.places_for(name);
        let stand = moves::stand(&core.coord, name);
        let moved = core.agents.get(name).is_some_and(|a| matches!(a.kind.as_str(), "claude" | "codex"));
        let notes: Vec<Option<String>> = tickets
            .iter()
            .map(|(d, _)| matches!(d.what, docket_machine::What::Step { .. }).then(|| m.layer_of(d.t)).flatten().filter(|l| Some(l) != stand.as_ref()).and_then(|l| moves::note(&places, &l, stand.as_deref(), moved)))
            .collect();
        // a step handed again in its layer after a move there (`sN.back`, paper-research N-101 p629): where the agent now
        // is first, as the notice of a move says it (`Core::arrived`), then the step itself
        let dir = core.agents.get(name).and_then(|a| a.session.get("cwd")).and_then(Value::as_str).unwrap_or_default().to_string();
        let arrivals: Vec<Option<String>> = tickets.iter().map(|(d, _)| d.id.ends_with(".back").then(|| stand.as_deref().and_then(|l| core.arrived(name, l, &dir))).flatten()).collect();
        // each thread's head, for its part's head line in a message joining several
        let heads: Vec<String> = tickets.iter().map(|(d, _)| core.ticket_head(d.t)).collect();
        (places, core.ticket_given_since(name), m.open_on(name).into_iter().map(|t| m.id(t)).collect::<Vec<String>>(), mute, notes.into_iter().zip(arrivals).collect::<Vec<_>>(), untitled, heads)
    };
    // parts of the turn's last message are read only when all it holds is posts (`Machine::reply_threads`): a step,
    // words or a notice among them, and the footer offers none (N-807.p4)
    let no_parts = tickets.iter().any(|(d, _)| !render::a_post(&d.id)) || held.iter().any(|id| !render::a_post(id));
    // SINGLE: what NAME holds now (handed now, and given since its last turn ended) is on one ticket, and so is every
    // open or resting ticket it takes part in (`Machine::open_on`, paper-research m1264): only then do the owner's words
    // go without an id (design-two-systems §4), found again by position (`Core::bare_handed`). An agent on two threads
    // is given words placed on one of them as posts, the ticket in the tag
    let mut on: std::collections::BTreeSet<&str> = tickets.iter().map(|(d, _)| d.ticket.as_str()).collect();
    on.extend(held.iter().filter_map(|id| id.split_once('.').map(|(t, _)| t)));
    let bare_pending = shared.lock().await.bare_pending(name);
    // a one-off (`give`, What::Words) is the owner's words whatever else NAME is on: its rule is as before (m1264)
    let single_words = on.len() == 1 && !bare_pending;
    on.extend(open_on.iter().map(String::as_str));
    let single = on.len() == 1 && !bare_pending;
    // which go in one message: into a turn, the owner's posts; out of it, all. Only when they are on two or more
    // tickets, so none goes without an id (that needs SINGLE, everything on one ticket)
    let owners = |d: &docket_machine::Delivery| matches!(&d.what, docket_machine::What::Posts { unread } if unread.iter().any(|p| p.by == "owner" && p.held.is_none() && !p.dropped));
    let join: Vec<bool> = tickets.iter().map(|(d, _)| !into || owners(d)).collect();
    let joined_on: std::collections::BTreeSet<&str> = tickets.iter().zip(&join).filter(|(_, j)| **j).map(|((d, _), _)| d.ticket.as_str()).collect();
    let batch = joined_on.len() >= 2;
    // delivery I rendered, with its note; JOINS: without its footer, for a message joining several. None: the agent's
    // renderings have no template for it
    let one = |i: usize, joins: bool| -> Result<Option<render::Entry>, String> {
        let (d, t) = &tickets[i];
        let single = if matches!(d.what, docket_machine::What::Words { .. }) { single_words } else { single };
        let hints = render::Hints { no_parts, untitled: untitled[i], batch: joins };
        let Some(mut e) = render::entry_in(&places, d, t, replies_on == Some(d.ticket.as_str()), single, again.contains(&d.id), false, mute[i].as_deref(), into, hints)? else { return Ok(None) };
        if let Some(n) = &notes[i].0 {
            e.text = format!("{n}\n\n{}", e.text);
        }
        Ok(Some(e))
    };
    let arrive = |i: usize, mut e: render::Entry| {
        if let Some(a) = &notes[i].1 {
            e.text = format!("{a}\n\n{}", e.text);
        }
        e
    };
    // what the first message begins with: the warning, then NOTICES (one line each, a blank line, then the text)
    let lead = |text: String, notices: &[String]| {
        let head: Vec<String> = warning.filter(|w| !w.is_empty()).map(String::from).into_iter().chain((!notices.is_empty()).then(|| notices.join("\n"))).collect();
        if head.is_empty() { text } else { format!("{}\n\n{text}", head.join("\n\n")) }
    };
    let mut out: Vec<(String, Value)> = Vec::new();
    let mut parts: Vec<(usize, render::Part)> = Vec::new();
    let mut joined_at: Option<usize> = None;
    for (i, (d, _)) in tickets.iter().enumerate() {
        let joins = batch && join[i];
        let first = out.is_empty() && joined_at.is_none();
        let e = match one(i, joins) {
            Ok(Some(e)) if joins => e,
            Ok(Some(mut e)) => {
                if first {
                    e.text = lead(e.text, notices);
                }
                arrive(i, e)
            }
            // not sent: its renderings have no template for it. A step fails at once with NoCall, so no one waits on an
            // agent never told of it
            Ok(None) => {
                if let docket_machine::What::Step { step, .. } = &d.what {
                    shared.lock().await.ticket_unrendered(name, &d.id, d.t, *step, format!("{name}'s renderings have no template for it"));
                }
                continue;
            }
            Err(e) => {
                eprintln!("coordd: tickets: rendering {}: {e}", d.id);
                if let docket_machine::What::Step { step, .. } = &d.what {
                    shared.lock().await.ticket_unrendered(name, &d.id, d.t, *step, e);
                    continue;
                }
                let text = format!("(coordd could not render this: {e})\n{}", serde_json::to_string(&d.what).unwrap_or_default());
                render::Entry { text, meta: json!({"id": d.id, "ticket": d.ticket, "kind": d.kind, "from": "coord"}), foot: Default::default() }
            }
        };
        if joins {
            joined_at.get_or_insert(out.len());
            parts.push((i, render::Part { ticket: d.ticket.clone(), label: heads[i].clone(), entry: e }));
        } else if e.meta.get("id").is_none() {
            shared.lock().await.bare_handed(name, &d.id, &e.text);
            out.push((e.text, e.meta));
        } else {
            out.push((e.text, e.meta));
        }
    }
    let Some(at) = joined_at else { return out };
    let whole: Vec<render::Part> = parts.iter().map(|(_, p)| p.clone()).collect();
    let message = if whole.len() >= 2 {
        let ids: Vec<&str> = whole.iter().filter_map(|p| p.entry.meta.get("id").and_then(Value::as_str)).collect();
        render::handover(&places, &whole, codex, if at == 0 { notices } else { &[] }).map_err(|e| eprintln!("coordd: tickets: rendering the hand-over of {}: {e}", ids.join(" "))).ok()
    } else {
        None
    };
    // one message, the notices under its head; or each as it goes alone: one left (the others were steps that did not
    // render), or no handover.tmpl
    let joined = message.is_some();
    let msgs: Vec<(String, Value)> = match message {
        Some(m) => vec![m],
        None => parts.iter().map(|(i, p)| one(*i, false).ok().flatten().map(|e| arrive(*i, e)).map(|e| (e.text, e.meta)).unwrap_or_else(|| (p.entry.text.clone(), p.entry.meta.clone()))).collect(),
    };
    for (k, (text, meta)) in msgs.into_iter().enumerate() {
        let text = if at == 0 && k == 0 { lead(text, if joined { &[] } else { notices }) } else { text };
        out.insert(at + k, (text, meta));
    }
    out
}

async fn run_job(shared: &Shared, job: Job) {
    match job {
        Job::Channel { name, ids, socks, mut line, tickets, replies_on, again, into, rest, digest } => {
            if !tickets.is_empty() {
                let shown = rendered_all(shared, &name, &tickets, replies_on.as_deref(), &again, into, false, &rest, &digest).await;
                let mut v: Value = serde_json::from_str(&line).unwrap_or_else(|_| json!({"events": []}));
                if let Some(events) = v.get_mut("events").and_then(Value::as_array_mut) {
                    for (content, meta) in shown {
                        events.push(render::event(content, meta));
                    }
                    if events.is_empty() {
                        // nothing left to give: every step failed to render
                        shared.lock().await.handed(&name, &ids, true, "channel");
                        return;
                    }
                }
                line = format!("{v}\n");
            }
            let talk = async {
                use tokio::io::AsyncReadExt;
                // the first socket whose server answers (`core::channel_socks`)
                let mut c = None;
                let mut tried = Vec::new();
                for sock in &socks {
                    match UnixStream::connect(sock).await {
                        Ok(x) => {
                            c = Some((sock.rsplit('/').next().unwrap_or(sock).to_string(), x));
                            break;
                        }
                        Err(e) => tried.push((sock.clone(), e)),
                    }
                }
                // no socket answered: the session's coord server is not running (true), which marks it unreached
                let Some((at, mut c)) = c else { return Err((core::channel_unreached(&tried), true)) };
                c.write_all(line.as_bytes()).await.map_err(|e| (format!("{at} took the connection and not the mail: {e}"), false))?;
                let mut buf = [0u8; 16];
                let n = c.read(&mut buf).await.map_err(|e| (format!("{at} gave no answer: {e}"), false))?;
                if buf[..n].starts_with(b"ok") {
                    Ok(())
                } else if n == 0 {
                    Err((format!("{at} closed without answering: the session's coord server ended"), false))
                } else {
                    Err((format!("{at} answered {:?}, not ok", String::from_utf8_lossy(&buf[..n])), false))
                }
            };
            // waited for while the server's socket is open: a server that ends closes it and the read fails, not taken
            // (no time limit: the owner, N-68)
            let r = talk.await;
            if let Err((why, _)) = &r {
                eprintln!("coordd: {name}: the hand-over of {} did not reach its session: {why}", ids.join(" "));
            }
            let mut core = shared.lock().await;
            if let Err((why, true)) = &r {
                core.unreached(&name, why);
            }
            core.handed(&name, &ids, r.is_ok(), "channel");
        }
        Job::Codex { name, ids, rpc, session, socket, mut text, tickets, replies_on, again, effort, into, rest, digest } => {
            for body in served_parts(shared, &name, &tickets, replies_on.as_deref(), &again, into, &rest, &digest).await {
                text = if text.is_empty() { body } else { format!("{text}\n\n{body}") };
            }
            if text.is_empty() {
                shared.lock().await.handed(&name, &ids, true, "codex");
                return;
            }
            // the owner's post into the running turn (N-101 p1214): `next` steers it in; its turn has begun already
            let (prio, how) = if into { ("next", "codex-next") } else { ("later", "codex") };
            let took = core::codex_rpc_at(&rpc, &session, &socket, &["send", prio, &text], effort.as_deref()).await;
            shared.lock().await.handed(&name, &ids, took, how);
        }
        // a fresh step's short-lived agent (plan/delta-m613.md): started where the work is, with its call's prompt
        // (paper-research m971: 10, 11); its step then comes to it as any agent's does, and its final output answers it
        Job::Start { agent, call, view, layer, from, cut } => {
            // a start that fails says so to the machine: the step owed to the agent raises NoWorker (N-101)
            let (t, id) = (view.n, view.id.clone());
            if let Err(why) = start_agent(shared, &agent, &call, view, layer, from, cut).await {
                eprintln!("coordd: tickets: {agent} for {call} on {id}: not started: {why}");
                if let Err(e) = shared.lock().await.ticket_input(docket_machine::Input::StartFailed { t, agent: agent.clone(), why }) {
                    eprintln!("coordd: tickets: {agent}: its failed start not taken: {e}");
                }
            }
        }
        // its step answered: the texts the ticket holds by reference in its transcript are copied (the one copy texts
        // get, owner m613: its transcript may go), then its session ends
        Job::End { agent, view } => {
            let coord = shared.lock().await.places().coord.clone();
            let mut locs: Vec<docket_machine::Loc> = Vec::new();
            if let Some(v) = &view {
                for p in v.posts.iter().filter(|p| p.by == agent) {
                    if let docket_machine::Text::Loc { loc } = &p.text {
                        locs.push(loc.clone());
                    }
                }
                locs.extend(v.given.iter().filter(|g| g.to == agent).filter_map(|g| g.place.clone()));
            }
            for loc in locs {
                match pin(&coord, &agent, &loc) {
                    Ok(to) => shared.lock().await.ticket_pinned(loc, to),
                    Err(e) => eprintln!("coordd: tickets: pinning {agent}'s text at {}:{}: {e}", loc.file, loc.offset),
                }
            }
            let base = shared.lock().await.base.clone();
            match tokio::process::Command::new(format!("{base}/bin/roster")).args(["down", &agent]).output().await {
                Ok(o) if o.status.success() => {
                    let dropped = shared.lock().await.ticket_drop_ended(&agent);
                    let also = if dropped.is_empty() { String::new() } else { format!("; {} dropped", dropped.join(" ")) };
                    // its cgroups, empty once its processes have ended, go with it (a boxed agent's limits are on them)
                    cgroup::forget(&agent);
                    eprintln!("coordd: tickets: {agent} done and stopped{also}")
                }
                Ok(o) => eprintln!("coordd: tickets: {agent}: roster down failed: {}", String::from_utf8_lossy(&o.stderr).trim()),
                Err(e) => eprintln!("coordd: tickets: {agent}: roster down: {e}"),
            }
        }
        // a layer's view made (SPEC) or ended (none): coordd's ticket module asks as layers become active or end
        Job::Layer { id, spec } => {
            let coord = shared.lock().await.places().coord.clone();
            let now = core::now();
            let opening = spec.is_some();
            let id2 = id.clone();
            let r = tokio::task::spawn_blocking(move || match spec {
                Some(Ok(spec)) => layer::open(&coord, &spec, &now).map(|l| format!("{id2} open, view {} on {} ({})", l.view.unwrap_or_default(), l.branch, l.base)),
                Some(Err(e)) => Err(e),
                None => layer::end(&coord, &id2, &now).map(|l| format!("{id2} ended; {} read-only", l.up)),
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            match r {
                Ok(said) => eprintln!("coordd: layers: {said}"),
                // a layer that cannot be made: its scope raises NoLayer (N-89.p4)
                Err(e) if opening => {
                    eprintln!("coordd: layers: {id}: {e}");
                    if let Err(e2) = shared.lock().await.ticket_input(docket_machine::Input::LayerFailed { layer: id.clone(), why: e }) {
                        eprintln!("coordd: layers: {id}: {e2}");
                    }
                }
                Err(e) => eprintln!("coordd: layers: {id}: {e}"),
            }
        }
        // layers whose tickets closed: removed, one after the other, what each did and what failed in the journal
        Job::LayersRemove { ids } => {
            let coord = shared.lock().await.places().coord.clone();
            let _ = tokio::task::spawn_blocking(move || {
                for (id, top) in ids {
                    match layer::remove(&coord, &id, top, &core::now()) {
                        Ok(said) => eprintln!("coordd: layers: {said}"),
                        Err(e) => eprintln!("coordd: layers: {id}: removing: {e}"),
                    }
                }
            })
            .await;
        }
        Job::Program { name, deliveries } => {
            // the runner: each check step to the project's runner command; its verdict comes back as a check_end event
            let runner = shared.lock().await.project.as_ref().and_then(|p| Some((p.dir.clone(), p.project.runner.clone()?)));
            // a boxed layer's commits reach the project's repository, where the runner reads them, before it is asked
            // (an agent's check of its own commit in the middle of its turn comes this way)
            let coord = shared.lock().await.places().coord.clone();
            for e in tokio::task::spawn_blocking(move || layer::sync_open(&coord)).await.unwrap_or_default() {
                eprintln!("coordd: layers: {e}");
            }
            for (d, _) in deliveries {
                let place = shared.lock().await.runner_place(d.t);
                let sent = match &runner {
                    Some((dir, cmd)) => {
                        let (dir, cmd, d2) = (dir.clone(), cmd.clone(), d.clone());
                        tokio::task::spawn_blocking(move || project::to_runner(&dir, &cmd, &d2, &place)).await.unwrap_or_else(|e| Err(project::NotSent { journal: e.to_string(), why: e.to_string(), ran: false }))
                    }
                    None => Err(project::NotSent { journal: "the project file names no runner".into(), why: "the project file names no runner".into(), ran: false }),
                };
                let mut core = shared.lock().await;
                // could not run at all: held back, not given (Core::runner_outcome)
                if !core.runner_outcome(&name, &d, &sent) {
                    continue;
                }
                // given, never resent; a step the runner refused raises RunnerRefused with its message, so it does not
                // stand waiting in silence (N-159 post 43)
                core.ticket_given(&name, &d);
                if let (Err(n), docket_machine::What::Step { step, again: false, .. }) = (&sent, &d.what) {
                    if let Err(e) = core.ticket_input(docket_machine::Input::RunnerRefused { t: d.t, step: *step, why: n.why.clone() }) {
                        eprintln!("coordd: tickets: {}: {e}", d.id);
                    }
                }
            }
        }
        Job::Host { name, deliveries } => {
            // a host session (coord-relay's): each delivery as mail to it was, a line in the feed the relay reads and
            // posts into the session; given when written, the relay holding what it cannot deliver yet
            let places = shared.lock().await.places();
            for (d, view) in deliveries {
                let (content, meta) = match render::delivery(&places, &d, &view, false, false, false, true, None) {
                    Ok(r) => r,
                    Err(e) => (format!("(coordd could not render this: {e})\n{}", serde_json::to_string(&d.what).unwrap_or_default()), json!({"from": "coord"})),
                };
                shared.lock().await.to_host(&name, &d, &content, &meta);
            }
        }
        Job::Clerk { deliveries } => {
            // the clerk, which coordd is: a filing's `file` step answered from one stateless split call (clerk.rs),
            // the passages made spans of the owner's text; a notice to the clerk needs nothing
            for (d, _) in deliveries {
                let Some(job) = shared.lock().await.clerk_job(&d) else { continue };
                // the call stopped (a panic): the clerk's filing is left Unplaced, the watcher's intake a one-off (a thread
                // refuses Unplaced; design-two-systems §3, a failed intake is a one-off)
                // and the watcher's route as a failed call is, why on its answer line (`clerk::route_failed`, design §13)
                let failed = |e: tokio::task::JoinError| match (job.worker.as_str(), &job.route) {
                    ("clerk", _) => (json!({"head": "Unplaced", "why": format!("the clerk stopped: {e}")}), None, None),
                    (_, Some(r)) => (clerk::route_failed(r), None, Some(format!("the routing call stopped: {e}"))),
                    _ => (json!({"head": "OneOff"}), None, None),
                };
                let (head, decision, fail) = tokio::task::spawn_blocking({ let job = job.clone(); move || clerk::answer(&job) }).await.unwrap_or_else(failed);
                shared.lock().await.clerk_answered(&job, head, decision, fail);
            }
        }
        Job::Deliver { delivery, .. } => {
            // a delivery run's `fold` (fold.call's `choose`, delivery.rs), or a pass of an agent's brain (brain.call,
            // brain.rs): one call, off the lock; one that stops (a panic) is the call failed: for `fold` everything
            // waiting goes, for the brain nothing is said. The lock let go before the else takes it again
            let job = {
                let mut core = shared.lock().await;
                match core.fold_job(&delivery) {
                    Some(j) => Some(j),
                    None => core.brain_job(&delivery),
                }
            };
            let Some(job) = job else {
                shared.lock().await.tickets.handed.remove(&(delivery.to.clone(), delivery.id.clone()));
                return;
            };
            if let Some(n) = job.fold.clone() {
                // how long the agent's mail waits on the call, from its start to its answer (paper-research N-1446 p2115)
                let started = std::time::Instant::now();
                let (head, decision, fail) = tokio::task::spawn_blocking({ let job = job.clone(); move || clerk::answer(&job) })
                    .await
                    .unwrap_or_else(|e| { let why = format!("the call stopped: {e}"); (clerk::fold_fell(&n, &why), None, Some(why)) });
                let took = started.elapsed();
                shared.lock().await.fold_answered(&job, &delivery, head, decision.and_then(|d| d.chosen), fail, Some(took));
                return;
            }
            let (head, decision, fail) = tokio::task::spawn_blocking({ let job = job.clone(); move || clerk::answer(&job) })
                .await
                .unwrap_or_else(|e| (clerk::brain_failed(&format!("the call stopped: {e}")), None, Some(format!("the call stopped: {e}"))));
            shared.lock().await.brain_answered(&job, &delivery, head, decision, fail);
        }
        Job::Minder { deliveries } => {
            // the minder, where nobody holds the role: one stateless call per shape step (shape.rs); a panic is Rest
            for (d, v) in deliveries {
                let Some(job) = shared.lock().await.minder_job(&d, &v) else { continue };
                let (head, decision, fail) = tokio::task::spawn_blocking({ let job = job.clone(); move || shape::answer(&job) }).await.unwrap_or_else(|_| (json!({"head": "Rest"}), None, None));
                shared.lock().await.minder_answered(&job, head, decision, fail);
            }
        }
        Job::Coord { jobs } => {
            // the text-change kind's steps (change.rs): a check read off the lock and held to the open tickets under it;
            // a load under the lock whole, so nothing opens or reloads between its write, its commit and its take; a
            // still drawn off it
            for job in jobs {
                if job.call == change::CHECK {
                    let checked = tokio::task::spawn_blocking({ let job = job.clone(); move || change::check(&job) })
                        .await
                        .unwrap_or_else(|e| change::Checked::Refused(json!({"head": "Refused", "why": format!("the check stopped: {e}"), "at": []})));
                    shared.lock().await.text_checked(&job, checked);
                } else if job.call == change::STILL {
                    // the still off the lock: the stand-in window started and waited for while it lives, the look sent
                    // again under the lock once its link is up (N-412 p1203)
                    let stand = shared.lock().await.text_still(&job);
                    let head = match stand {
                        Err(head) => head,
                        Ok(stand) => tokio::task::spawn_blocking({
                            let shared = shared.clone();
                            move || change::draw(&stand, || shared.blocking_lock().text_still_look())
                        })
                        .await
                        .unwrap_or_else(|e| json!({"head": "Unseen", "why": format!("the still stopped: {e}")})),
                    };
                    shared.lock().await.text_answer(&job, head);
                } else {
                    shared.lock().await.text_load(&job);
                }
            }
        }
        Job::Waits => shared.lock().await.refresh_waits().await,
        Job::Interrupt { name, pty, codex } => {
            match codex {
                Some((rpc, session, socket)) => {
                    core::codex_rpc(&rpc, &session, &socket, &["interrupt"]).await;
                }
                None => {
                    pty_send(std::path::Path::new(&pty), coord_types::pty::KEYS, b"\x1b").await;
                }
            }
            shared.lock().await.interrupted(&name);
        }
        Job::Command { name: _, pty, line } => {
            // the holder keeps it and types it itself once no turn runs and the input box is empty (coord-pty)
            pty_send(std::path::Path::new(&pty), coord_types::pty::COMMAND, line.as_bytes()).await;
        }
        Job::Move(m) => moves::run(shared, m).await,
        Job::AccountSaid(keep) => {
            let store = shared.lock().await.accounts.clone();
            let (keep, kept) = match tokio::task::spawn_blocking(move || {
                let kept = keep.run(&store);
                (keep, kept)
            })
            .await
            {
                Ok(done) => done,
                Err(e) => return eprintln!("coordd: what a session said of its account's limits was not kept: {e}"),
            };
            shared.lock().await.account_kept(&keep, kept);
        }
        Job::AccountCheck { id } => {
            let r = account_check(shared, &id, false).await;
            eprintln!("coordd: account {id}: a turn on it was refused its login; Claude Code asked: {}", r.out);
        }
        Job::GoUsage { id } => loop {
            let store = shared.lock().await.accounts.clone();
            if let Err(why) = go_read(shared, &store, &id).await {
                eprintln!("coordd: account {id}: Go's usage was not read after a turn on it: {why}");
            }
            if !shared.lock().await.go_read_done(&id) {
                break;
            }
        },
        Job::LimitRow { name, transcript, config, since } => {
            // TRANSCRIPT is the hook's word: only a transcript under CONFIG's `projects` is opened (`row_of`)
            let folder = config.as_str().map(std::path::PathBuf::from);
            let read = tokio::task::spawn_blocking(move || accounts::limits::row_of(&folder?, &transcript, since)).await;
            if let Ok(Some(mut row)) = read {
                row["claude_config_dir"] = config;
                // what is to be written is said under the lock; the write is `Job::AccountSaid`'s, off it
                let mut core = shared.lock().await;
                if let Some(keep) = core.account_said(&name, "limit_row", &row) {
                    core.account_keep(keep);
                }
            }
        }
        Job::ResetOffers { name, id } => {
            // drawn with the limit's error: looked for over three seconds and kept once found; a screen that shows none
            // at this limit names none (the card then offers `resets::AT_A_LIMIT`), and one that could not be read
            // leaves what was known
            let (coord, store) = {
                let c = shared.lock().await;
                (c.coord.clone(), c.accounts.clone())
            };
            let mut found: Option<Vec<coord_types::ResetOffer>> = None;
            for wait in [500, 1000, 1500] {
                tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
                let (c, n) = (coord.clone(), name.clone());
                let Some(screen) = tokio::task::spawn_blocking(move || screen_text(&c, &n)).await.ok().flatten() else { continue };
                found = Some(accounts::resets::offers(&screen, accounts::mover::now_s()).unwrap_or_default());
                if found.as_ref().is_some_and(|o| !o.is_empty()) {
                    break;
                }
            }
            let Some(offers) = found else { return eprintln!("coordd: {name}: its screen could not be read for the resets account {id} offers at its limit") };
            let said = accounts::resets::said(&offers);
            match account_update(&store, &id, move |s| accounts::resets::noticed(s, offers, accounts::mover::now_s())).await {
                Ok(()) => eprintln!("coordd: {name}: account {id} met its limit; Claude Code offers it {said}"),
                Err(why) => eprintln!("coordd: {name}: the resets account {id} offers were not kept: {why}"),
            }
            shared.lock().await.account_checked(&id);
        }
        Job::DialogLooked { name, title } => {
            // read as `coordc screen` reads it; no holder, or no answer: the hold stays, and the next minute reads again
            let coord = shared.lock().await.coord.clone();
            let n = name.clone();
            let shown = tokio::task::spawn_blocking(move || screen_text(&coord, &n)).await.ok().flatten();
            if shown.is_some_and(|text| !text.contains(title.as_str())) {
                shared.lock().await.dialog_gone(&name, &title);
            }
        }
        Job::Post { agent, t, text, answers } => {
            // the request its `coord post T TEXT` makes, from it: the same norms, post and notices
            let req = Request::Post { kind: None, t: t.clone(), text, to: None, answers, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
            let r = Box::pin(handle(shared, &Caller::Agent(agent.clone()), 0, req)).await;
            if !r.ok {
                eprintln!("coordd: tickets: {agent}'s last message: its part for {t} is not posted: {}", r.out);
            }
        }
        Job::Restart { name, pid } => {
            // `roster restart` stops the session's terminal and resumes it with session.json's MODEL:EFFORT, waited for
            // until it exits (no time limit: the owner, N-68); for a `fork` (part 4, `Live::fork_due`) on a copy of its
            // session cut where the fork goes on from, for the owner's edit of its system prompt with its version
            // (`Live::system_due`), as `restart_args` says
            let (base, cut, system) = {
                let core = shared.lock().await;
                let live = core.agents.get(&name).map(|a| &a.live);
                (core.base.clone(), live.and_then(|l| l.fork_due.clone()), live.and_then(|l| l.system_due.clone()))
            };
            let mut cmd = tokio::process::Command::new(format!("{base}/bin/roster"));
            cmd.args(restart_args(&name, cut.as_ref(), system.as_ref())).stdin(std::process::Stdio::null()).kill_on_drop(true);
            let why = match cmd.output().await {
                Ok(o) if o.status.success() => {
                    match (&cut, &system) {
                        (Some(c), _) => eprintln!("coordd: {name} restarted on a copy of its session cut where {} was given", c.place),
                        (None, Some(s)) => eprintln!("coordd: {name} restarted with system prompt {}{}", s.version, s.cut.as_ref().map(|c| format!(", on a copy of its session cut at {}", c.place)).unwrap_or_default()),
                        (None, None) => eprintln!("coordd: {name} restarted for its effort"),
                    }
                    String::new()
                }
                Ok(o) => format!("roster restart failed: {}", last_line(&o.stderr)),
                Err(e) => format!("roster restart: {e}"),
            };
            let mut core = shared.lock().await;
            core.restarted(&name, pid, &why);
            // the owner's edit that did not take: roster's words on the agent, for his window (N-1788)
            if let Some(s) = &system {
                core.system_applied(&name, s.version, &why);
            }
        }
    }
}

/// roster's arguments for NAME's restart (`Job::Restart`): `restart NAME`; with FORK's cut, `--cut FILE UPTO` (part 4);
/// with the owner's edit of its system prompt SYSTEM, `--system VERSION append|replace`, and its own cut when he picked
/// a turn to go on from. Not both cuts: an edit is refused while a fork is due (`Core::system_prompt_from`), and nothing is
/// handed while an edit is (`Core::hand_over`), so no fork is decided before its restart.
fn restart_args(name: &str, fork: Option<&crate::core::ForkCut>, system: Option<&crate::core::SystemDue>) -> Vec<String> {
    let mut args = vec!["restart".to_string(), name.to_string()];
    if let Some(c) = fork.or(system.and_then(|s| s.cut.as_ref())) {
        args.extend(["--cut".to_string(), c.file.clone(), c.upto.to_string()]);
    }
    if let Some(s) = system {
        args.extend(["--system".to_string(), s.version.to_string(), (if s.replace { "replace" } else { "append" }).to_string()]);
    }
    args
}

/// NAME's screen as text, from its terminal holder, as `coordc screen` reads it; None with no holder or no answer.
fn screen_text(coord: &str, name: &str) -> Option<String> {
    use std::io::Write;
    let mut s = std::os::unix::net::UnixStream::connect(coord_types::pty::socket(coord, name)).ok()?;
    s.write_all(&coord_types::pty::frame(coord_types::pty::TEXT, &[])).ok()?;
    match coord_types::pty::read(&mut s) {
        Ok((coord_types::pty::TEXT, p)) => Some(String::from_utf8_lossy(&p).into_owned()),
        _ => None,
    }
}

/// The reasoning efforts Codex's model MODEL takes, from Codex's own catalog (`codex debug models`: each model's
/// `supported_reasoning_levels`), or why they are not known. `codex` as roster runs it (agent-base/bin first on PATH).
async fn codex_efforts(model: &str) -> Result<Vec<String>, String> {
    let mut cmd = tokio::process::Command::new("codex");
    cmd.args(["debug", "models"]).stdin(std::process::Stdio::null()).kill_on_drop(true);
    let out = match tokio::time::timeout(std::time::Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(o)) if o.status.success() => o.stdout,
        Ok(Ok(o)) => return Err(format!("codex debug models failed: {}", last_line(&o.stderr))),
        Ok(Err(e)) => return Err(format!("codex debug models: {e}")),
        Err(_) => return Err("codex debug models did not end in 30 s".into()),
    };
    codex_model_efforts(&out, model)
}

/// A command's error in one line: the last line of its STDERR that says something.
fn last_line(stderr: &[u8]) -> String {
    let s = String::from_utf8_lossy(stderr);
    s.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("(nothing on stderr)").to_string()
}

/// MODEL's efforts in Codex's catalog CATALOG (`codex debug models`' JSON).
fn codex_model_efforts(catalog: &[u8], model: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_slice(catalog).map_err(|e| format!("codex debug models: {e}"))?;
    let m = v["models"].as_array().into_iter().flatten().find(|m| m["slug"] == model).ok_or_else(|| format!("Codex's catalog has no model {model}"))?;
    let levels: Vec<String> = m["supported_reasoning_levels"].as_array().into_iter().flatten().filter_map(|l| l["effort"].as_str().map(str::to_string)).collect();
    if levels.is_empty() {
        return Err(format!("Codex's catalog names no levels for {model}"));
    }
    Ok(levels)
}

/// Where a fresh agent runs (paper-research m971, 10): the repository of its ticket's context (`ctx.repo`) if it has
/// one, else the project's folder REPO when the project file sets it (`repo`, N-89.p4), else the folder most of the
/// project's agents run in (CWDS; the New agent dialog's rule, ties to the shorter path), never the project's files
/// folder FILES. None: no such folder.
fn fresh_dir(ctx: &Value, repo: Option<&str>, cwds: &[String], files: Option<&str>) -> Option<String> {
    let not_files = |d: &str| files.is_none_or(|f| d.trim_end_matches('/') != f.trim_end_matches('/'));
    if let Some(r) = ctx.get("repo").and_then(Value::as_str).filter(|r| !r.is_empty() && not_files(r)) {
        return Some(r.to_string());
    }
    if let Some(r) = repo {
        return Some(r.to_string());
    }
    let mut n: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for d in cwds.iter().map(String::as_str).filter(|d| not_files(d)) {
        *n.entry(d).or_default() += 1;
    }
    n.into_iter().max_by_key(|(d, k)| (*k, std::cmp::Reverse(*d))).map(|(d, _)| d.to_string())
}

/// Start scoped agent AGENT for step CALL of ticket VIEW (Job::Start): a fresh one, a fork of FROM, or one that ran
/// before on its own conversation, in LAYER's view and box if any. Why it was not started, otherwise.
///
/// Its model (the model classes, models.rs): one that ran before keeps its own; a fork keeps FROM's, refused if it does
/// not meet CALL's `where`; a fresh one is the policy's `model-for` among the models that meet it, recorded on the
/// ticket as a `decided` line. None is taken by default: with no `where` and no model-for rule of the project for
/// agents, it is not started.
async fn start_agent(shared: &Shared, agent: &str, call: &str, view: docket_machine::TicketView, layer: Option<String>, from: Option<String>, cut: Option<String>) -> Result<(), String> {
    enum Pick {
        Keep(String),
        Copy(String, String),
        Decide,
    }
    let t = view.n;
    let (dir, prompt, pick, cond, places, served, step) = {
        let core = shared.lock().await;
        let places = core.places();
        // a copy (CUT, N-1104 p1154) runs in its agent's own folder, where a resume finds the cut copy of its session
        let own = cut.as_ref().and(from.as_deref()).and_then(|f| core.agents.get(f)).and_then(|a| a.session.get("cwd").and_then(Value::as_str).map(str::to_string));
        let dir = match own.or_else(|| core.start_dir(&view.ctx)) {
            Some(d) => d,
            None => crate::homes::made(&core.coord, agent)?,
        };
        let prompt = fresh_prompt(&places.call_dirs(), &places.base, call);
        let cond = core.tickets.m.call_of(t, call).and_then(|s| s.cond);
        let pick = if let Some(a) = core.agents.get(agent) {
            // started again: its own conversation, on its own model
            Pick::Keep(a.model.clone())
        } else if std::path::Path::new(&format!("{}/agents/{agent}/session.json", places.coord)).is_file() {
            Pick::Keep(String::new())
        } else if let Some(f) = from.as_deref() {
            match core.agents.get(f) {
                Some(a) => Pick::Copy(f.to_string(), a.model.clone()),
                None => return Err(format!("{f} is not a registered agent, so there is nothing to copy")),
            }
        } else {
            Pick::Decide
        };
        let step = core.tickets.m.tickets.get(&t).map_or(0, |x| x.steps);
        (dir, prompt, pick, cond, places, core.models().to_vec(), step)
    };
    let model = match pick {
        Pick::Keep(m) => m,
        Pick::Copy(f, m) => {
            if let Some(c) = cond.clone() {
                let (f2, m2, call2) = (f.clone(), m.clone(), call.to_string());
                tokio::task::spawn_blocking(move || models::copy_meets(&places, &served, &f2, &m2, &call2, &c)).await.map_err(|e| e.to_string())??;
            }
            m
        }
        Pick::Decide => {
            let (call2, agent2, cond2) = (call.to_string(), agent.to_string(), cond.clone());
            let (d, effort) = tokio::task::spawn_blocking(move || -> Result<_, String> { Ok((models::decide(&places, &agent2, call2.as_str(), cond2.as_deref())?, models::effort_of(&places, &call2)?)) }).await.map_err(|e| e.to_string())??;
            shared.lock().await.ticket_decision(t, step, &d);
            let m = match d.chosen {
                Some(m) => m,
                None => return Err(format!("no model for call {call}: {}", models::why_none(&d))),
            };
            // the effort the policy gives the step's call (`effort-for`), on the model chosen: `claude-opus-5-5:medium`
            match effort {
                None => m,
                Some(level) => {
                    let family = served.iter().find(|(n, _)| *n == m).map(|(_, f)| f.clone());
                    let codex = if family.as_deref() == Some("codex") { Some(codex_efforts(&m).await) } else { None };
                    models::at_effort(call, &m, family.as_deref(), &level, codex)?
                }
            }
        }
    };
    let what = format!("for {call} on {}", view.id);
    launch(shared, Launch { agent: agent.to_string(), model, dir, prompt, layer, from, cut, what, force: false }).await
}

/// `Request::Record` (consult-3 N-1446 p34): which tickets and from when, under the core lock, a moment; the record's
/// file, research's 140 MB, read off it on a blocking thread, so no agent's delivery waits on the scan.
async fn record_off_lock(shared: &Shared, t: &str, at: Option<u32>, after: Option<u64>, cap: u32) -> Reply {
    let scope = shared.lock().await.ticket_record(t, at);
    match scope {
        Ok(scope) => tokio::task::spawn_blocking(move || ticket::record_lines(&scope, after, cap)).await.unwrap_or_else(|e| err(format!("coordd: {e}"))),
        Err(r) => r,
    }
}

/// `Request::Raw` (proto-window N-1446 p1383; paper-research p1380), the owner's: node ID's words from its source. A
/// slice of a session: its giving under the lock, a moment ([`core::Core::ticket_slice`]), the record's line and the
/// session's bytes read off it on a blocking thread ([`ticket::slice_words`]). A system prompt block's text node
/// (p1642, 2): where the shape of the request that carried it starts, under the lock ([`core::Core::ticket_block`]),
/// and that line read off it ([`ticket::block_words`]). Nothing of them is recorded or logged.
async fn raw_off_lock(shared: &Shared, w: &Who, id: &str) -> Reply {
    if id.starts_with("text:") {
        let scope = shared.lock().await.ticket_block(w, id);
        return match scope {
            Ok(s) => match tokio::task::spawn_blocking(move || ticket::block_words(&s)).await {
                Ok(Ok(text)) => ok(text),
                Ok(Err(why)) => err(format!("coordd: {why}")),
                Err(e) => err(format!("coordd: {e}")),
            },
            Err(why) => err(format!("coordd: {why}")),
        };
    }
    let scope = shared.lock().await.ticket_slice(w, id);
    match scope {
        Ok(s) => match tokio::task::spawn_blocking(move || ticket::slice_words(&s)).await {
            Ok(Ok(text)) => ok(text),
            Ok(Err(why)) => err(format!("coordd: {why}")),
            Err(e) => err(format!("coordd: {e}")),
        },
        Err(why) => err(format!("coordd: {why}")),
    }
}

/// `Request::Layers` (Home's Layers now, a2h-speed N-1446 p2815): the layers' registry, the runner's running replays'
/// layers from its job files, and each open layer's own folder measured, all off the lock (`layer::answer`).
async fn layers_off_lock(shared: &Shared) -> Reply {
    let coord = shared.lock().await.places().coord;
    tokio::task::spawn_blocking(move || {
        let runs = jobs::run_layers(&jobs::dir(), jobs::alive);
        ok(serde_json::to_string(&layer::answer(layer::load(&coord), runs)).unwrap_or_default())
    })
    .await
    .unwrap_or_else(|e| err(format!("coordd: {e}")))
}

/// `Request::Insight` for insight (1), the owner's (S6): the record's file named under the lock, a moment, and its
/// `turn_ended` lines read off it on a blocking thread (`ticket::insight_turns`), as [`record_off_lock`] reads it.
async fn insight_off_lock(shared: &Shared, w: &Who) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("an insight into the data is the owner's".into());
    }
    let file = shared.lock().await.record_file();
    tokio::task::spawn_blocking(move || ticket::insight_turns(&file)).await.unwrap_or_else(|e| err(format!("coordd: {e}")))
}

/// `Request::Subject`, `Harness`, `HarnessLatest`, `HarnessSet` (server_reads.rs), `Machines` (machine-state): the
/// owner's, each read or written off the lock; an agent's cgroup or box is refused.
async fn server_read(shared: &Shared, w: &Who, req: Request) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("the app's pages' reads on the server are the owner's".into());
    }
    let done = |r: Result<String, String>| match r {
        Ok(s) => ok(s),
        Err(e) => err(e),
    };
    match req {
        Request::Subject { hash, repo } => {
            let repo = {
                let core = shared.lock().await;
                server_reads::repo_for(core.project.as_ref(), repo.as_deref())
            };
            match repo {
                Ok(r) => done(tokio::task::spawn_blocking(move || server_reads::subject(&r, &hash)).await.unwrap_or_else(|e| Err(e.to_string()))),
                Err(e) => err(e),
            }
        }
        Request::Harness => {
            let coord = shared.lock().await.places().coord.clone();
            done(tokio::task::spawn_blocking(move || Ok(server_reads::harness(&coord).to_string())).await.unwrap_or_else(|e| Err(e.to_string())))
        }
        Request::HarnessLatest => done(tokio::task::spawn_blocking(server_reads::latest).await.unwrap_or_else(|e| Err(e.to_string()))),
        // the Harnesses page's switches (N-802.p23): coordd the rules file's one writer, off the lock
        Request::HarnessSet { harness, pairs, on, all_on } => {
            let coord = shared.lock().await.places().coord.clone();
            let change = if all_on { server_reads::Change::AllOn } else { server_reads::Change::Set(pairs, on) };
            done(tokio::task::spawn_blocking(move || server_reads::set_rules(&coord, &harness, change)).await.unwrap_or_else(|e| Err(e.to_string())))
        }
        // the Machines page's one file (N-1136 post 6), by its fixed name, off the lock; an old one starts one sample
        // beside the answer (N-412 posts 1958, 1963)
        Request::Machines => {
            let sampler = std::path::Path::new(&shared.lock().await.base).join("bin").join("machines-sample");
            done(
                tokio::task::spawn_blocking(move || {
                    machine_state::machines::folder().map(|f| {
                        let now = std::time::SystemTime::now();
                        machine_state::machines::refresh(&f, &sampler, now);
                        let a = machine_state::machines::read(&f, now);
                        // NativeLink's workers on their device, when compute.json names its listener
                        match compute_status::named(&f).and_then(|s| Some((s.port?, s))) {
                            Some((port, s)) => compute_status::on_devices(a, &s, compute_status::listed(port).map(|l| compute_status::workers(&l)).unwrap_or_default()).to_string(),
                            None => a.to_string(),
                        }
                    })
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string())),
            )
        }
        _ => err("coordd: not a read on the server".into()),
    }
}

/// `Request::Split` (split.rs): the owner's only; the open threads read under the lock, the call off it, on the model
/// the policy chooses for the task `split`; the policy's `split-points-at-most` and `split-threads-at-most` bound it.
async fn split_text(shared: &Shared, w: &Who, text: String) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("proposing a message's points is the owner's".into());
    }
    let (places, mut rows) = {
        let core = shared.lock().await;
        (core.places(), ticket::outline_rows(&core.tickets.m))
    };
    let r = tokio::task::spawn_blocking(move || {
        let scopes = places.sources().unwrap_or_default();
        let fact = |f: &str, or: usize| policy::number_fact(&scopes, f).map_or(or, |n| n as usize);
        let (points_at_most, threads_at_most) = (fact("split-points-at-most", split::POINTS), fact("split-threads-at-most", split::THREADS));
        // the open threads, the latest opened first
        rows.retain(|o| o.open);
        rows.sort_by(|a, b| b.opened_at.cmp(&a.opened_at));
        let threads: Vec<split::Thread> = rows.into_iter().take(threads_at_most).map(|o| split::Thread { id: o.id, label: o.label, on: o.on }).collect();
        split::split(&policy::Engine::start(places), &text, &threads, points_at_most)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(s) => ok(s),
        Err(e) => err(e),
    }
}

/// `Request::CleanText` (clean.rs): the owner's only; the call runs off the lock, on the model the policy chooses for
/// the task `clean`; the coord's agents' names, the project's `keep-words` and his KEEP are held to come back as they
/// are and are never a doubt, and are not sent.
async fn clean_text(shared: &Shared, w: &Who, text: String, keep: Vec<String>) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("cleaning a message's typing is the owner's".into());
    }
    let (places, names) = {
        let core = shared.lock().await;
        let places = core.places();
        let names = clean::kept_words(&places, core.agents.keys().cloned(), &keep);
        (places, names)
    };
    let r = tokio::task::spawn_blocking(move || clean::clean(&policy::Engine::start(places), &text, &names)).await.unwrap_or_else(|e| Err(e.to_string()));
    match r {
        Ok(s) => ok(s),
        Err(e) => err(e),
    }
}

/// The owner's New agent, Fork or Rename from the app (protocol 1, N-412 ask 13): what `roster` is run with, and the
/// prompt file a new agent's first words go to. Until protocol 1 the app ran these through a shell on the host
/// (`host-coordd sh` or `box.sh sh`), which an app on another machine cannot.
#[derive(Debug, PartialEq)]
struct OwnerRoster {
    args: Vec<String>,
    /// New agent: its first words, for `$COORD/prompts/NAME.md` (that file is the args' third).
    prompt: Option<(String, String)>,
    /// Fork: the copy is cut at the end of the last turn that ended by this time (seconds), not as it stands.
    by: Option<i64>,
}

impl OwnerRoster {
    fn of(req: &Request) -> Option<OwnerRoster> {
        let s = |x: &str| x.to_string();
        Some(match req {
            Request::AddAgent { name, model, text, dir } => OwnerRoster {
                args: vec![s("add"), name.clone(), model.clone(), format!("prompts/{name}.md"), dir.clone().unwrap_or_default()],
                prompt: Some((name.clone(), text.clone())),
                by: None,
            },
            Request::ForkAgent { from, new, model, dir, text, at } => {
                let mut args = vec![s("fork"), from.clone()];
                for (flag, v) in [("--name", new), ("--model", model), ("--dir", dir)] {
                    if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
                        args.extend([s(flag), v.clone()]);
                    }
                }
                args.extend(text.iter().filter(|t| !t.trim().is_empty()).cloned());
                OwnerRoster { args, prompt: None, by: *at }
            }
            Request::RenameAgent { name, to } => OwnerRoster { args: vec![s("rename"), name.clone(), to.clone()], prompt: None, by: None },
            _ => return None,
        })
    }

    /// Why a name in it cannot be an agent's (it names a file under the coord's folder, and roster's arguments).
    fn bad_name(&self) -> Option<String> {
        let names: Vec<&String> = match self.args[0].as_str() {
            "add" | "rename" => vec![&self.args[1]].into_iter().chain((self.args[0] == "rename").then(|| &self.args[2])).collect(),
            _ => self.args.windows(2).filter(|w| w[0] == "--name").map(|w| &w[1]).chain(std::iter::once(&self.args[1])).collect(),
        };
        names.into_iter().find(|n| n.is_empty() || n.starts_with(['.', '-']) || n.contains(|c: char| c == '/' || c.is_whitespace())).map(|n| format!("{n:?} is not an agent's name"))
    }
}

/// Run R's `roster` for the owner, as the app's shell did: in the coord's folder, with coordd's own environment (COORD,
/// AGENT_BASE), so the agent registers with the owner its parent; the reply is roster's last line, or why it failed.
/// No time limit: a rename waits for the agent's turn to end (the owner, N-68).
async fn owner_roster(shared: &Shared, w: &Who, r: OwnerRoster) -> Reply {
    if !matches!(w, Who::Owner) {
        return err(format!("coord {}: the owner's (an agent makes agents with coord new or coord fork)", r.args[0]));
    }
    if let Some(why) = r.bad_name() {
        return err(format!("coord {}: {why}", r.args[0]));
    }
    // where New agent starts it when the window names no folder (it names none from N-1446 p2455): coordd's one rule
    let (base, coord, start) = {
        let mut core = shared.lock().await;
        // a rename moves the agent's system prompts: refused while his edit of one waits for its restart, which would
        // find it gone; while it runs no edit is taken (`Core::system_prompt_from`; docket-core N-1446 p2471)
        if r.args[0] == "rename" {
            if core.agents.get(&r.args[1]).is_some_and(|a| a.live.system_due.is_some()) {
                return err(format!("coord rename: {} restarts with your edit of its system prompt first; rename it once it has", r.args[1]));
            }
            core.renaming.insert(r.args[1].clone());
        }
        (core.base.clone(), core.places().coord.clone(), core.start_dir(&json!({})))
    };
    let mut args = r.args.clone();
    // a fork of a busy agent (the owner's N-1794: "support forking a busy agent cleanly"): its copy is cut where every
    // tool call so far has its result (`fork::settled_end`), not in the middle of one, which its copy could not resume
    // on; an idle agent's settles at its transcript's end and is copied whole, as before. Not in a boxed project, whose
    // sessions are its agents' own users' (roster's cut copy is the host's)
    if r.args[0] == "fork" {
        let file = {
            let core = shared.lock().await;
            let boxed = core.project.as_ref().is_some_and(|p| p.project.boxed);
            core.agents.get(&r.args[1]).filter(|a| a.kind == "claude" && !boxed).and_then(|a| a.transcript())
        };
        // a rewind (the Map's Rewind / Branch from here): cut at the last turn that ended by its time
        if let Some(by) = r.by {
            let Some(file) = file else { return err(format!("coord fork: {}'s session cannot be cut at a past turn here; not forked", r.args[1])) };
            let f = file.clone();
            match tokio::task::spawn_blocking(move || crate::follow::turn_end_by(&f, by)).await.ok().flatten() {
                Some(n) => {
                    eprintln!("coordd: the owner's rewind of {}: cut after {n} bytes, its last turn's end by {by}", r.args[1]);
                    args.splice(2..2, ["--cut".to_string(), file, n.to_string()]);
                }
                None => return err(format!("coord fork: no turn of {} had ended by then; not forked", r.args[1])),
            }
        } else if let Some(file) = file {
            let f = file.clone();
            let cut = tokio::task::spawn_blocking(move || Some((crate::fork::settled_end(&f)?, std::fs::metadata(&f).ok()?.len()))).await.ok().flatten();
            match cut {
                Some((n, len)) if n < len => {
                    eprintln!("coordd: the owner's fork of {}: cut after {n} of {len} bytes, where every call has its result", r.args[1]);
                    args.splice(2..2, ["--cut".to_string(), file, n.to_string()]);
                }
                Some(_) => {}
                None => return err(format!("coord fork: {}'s session ({file}) has no point where its calls are all answered; not forked", r.args[1])),
            }
        }
    }
    if let Some((name, text)) = &r.prompt {
        let dir = format!("{coord}/prompts");
        let file = format!("{dir}/{name}.md");
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&file, format!("{text}\n"))) {
            return err(format!("coord add {name}: its first words could not be written ({file}: {e})"));
        }
        args[3] = file;
        if args[4].is_empty() {
            args[4] = match start {
                Some(d) => d,
                None => match crate::homes::made(&coord, name) {
                    Ok(h) => h,
                    Err(e) => return err(format!("coord add {name}: its home could not be made ({e})")),
                },
            };
        }
    }
    let mut cmd = tokio::process::Command::new(format!("{base}/bin/roster"));
    cmd.args(&args).current_dir(&coord).stdin(std::process::Stdio::null());
    let said = args.join(" ");
    let reply = match cmd.output().await {
        Ok(o) if o.status.success() => {
            eprintln!("coordd: the owner's roster {said}: done");
            ok(last_line(&o.stdout))
        }
        Ok(o) => {
            let why = last_line(&[o.stderr, o.stdout].concat());
            eprintln!("coordd: the owner's roster {said} failed ({}): {why}", o.status);
            err(format!("roster {} failed ({}): {why}", r.args[0], o.status))
        }
        Err(e) => err(format!("roster {}: {e}", r.args[0])),
    };
    if r.args[0] == "rename" {
        shared.lock().await.renaming.remove(&r.args[1]);
    }
    reply
}

/// What a spin-off is asked at its Join back (docs/archive/design/library-one's `sums_up`, the owner's walk item 19).
const SUMS_UP: &str = "You were split off from another agent to try something. It has not seen anything you did since. \
Say what it needs to go on: what you found, what you changed and where, and what you would do next. Nothing else.";

/// The owner's end of spin-off NAME (`Request::SpinEnd`; his walk item 19, N-1794: "either Join back (summarizing/
/// merging results into the parent), Close, or Delete"). NAME is a copy of another agent's conversation
/// (`AgentView::from`), not ended yet.
/// - Join: what it found, as one answer of a copy of its session to [`SUMS_UP`] (`roster ask`: its own session is not
///   touched, so a stopped spin-off is joined back too), goes to the agent it came from as its own message; then as
///   Delete. Refused while it is in a turn: what it says then would leave out what the turn is doing.
/// - Close: its session ends (`roster down`); it stays a stopped agent, which he can start again.
/// - Delete: its session ends and it is ended for good (`Core::set_ended`): in none of his lists, its sessions kept.
/// Nothing is done when a step fails: the answer says which.
async fn spin_end(shared: &Shared, w: &Who, name: String, how: coord_types::SpinEnd) -> Reply {
    use coord_types::SpinEnd;
    if !matches!(w, Who::Owner) {
        return err("coord spin: the owner's (he ends a spin-off with Join back, Close or Delete)".into());
    }
    let (from, base, coord, busy) = {
        let core = shared.lock().await;
        let Some(a) = core.agents.get(&name) else { return err(format!("coord spin: no agent {name}")) };
        if let Some(at) = &a.ended {
            return err(format!("coord spin: {name} was ended at {at}"));
        }
        let Some(from) = core.view(&name).from else { return err(format!("coord spin: {name} is no spin-off (a copy of no agent)")) };
        (from, core.base.clone(), core.places().coord.clone(), core.in_turn(a))
    };
    let roster = |args: Vec<String>| {
        let mut cmd = tokio::process::Command::new(format!("{base}/bin/roster"));
        cmd.args(args).current_dir(&coord).stdin(std::process::Stdio::null());
        cmd
    };
    if how == SpinEnd::Join {
        if busy {
            return err(format!("coord spin: {name} is in a turn; Join back once it has ended"));
        }
        let found = match roster(vec!["ask".into(), name.clone(), SUMS_UP.into()]).output().await {
            Ok(o) if o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Ok(o) if o.status.success() => return err(format!("coord spin: {name} said nothing to hand back; not joined")),
            Ok(o) => return err(format!("coord spin: {name} could not be asked what it found ({}): {}", o.status, last_line(&[o.stderr, o.stdout].concat()))),
            Err(e) => return err(format!("coord spin: roster ask: {e}")),
        };
        let body = format!("Joined back: {name}, your spin-off\n\nWhat {name} found, in its words (it was split off from you; the owner joined it back):\n\n{found}");
        let r = shared.lock().await.ticket_msg_under(&Who::Agent(name.clone()), None, vec![from.clone()], &body, false, None, None, Vec::new(), Default::default(), None, None, None);
        if !r.ok {
            return err(format!("coord spin: what {name} found could not be handed to {from}: {}", r.out));
        }
        eprintln!("coordd: the owner's Join back of {name}: handed to {from}");
    }
    let running = shared.lock().await.agents.get(&name).is_some_and(|a| a.running());
    if running {
        match roster(vec!["down".into(), name.clone()]).output().await {
            Ok(o) if o.status.success() => {}
            Ok(o) => return err(format!("coord spin: {name}'s session could not be ended ({}): {}", o.status, last_line(&[o.stderr, o.stdout].concat()))),
            Err(e) => return err(format!("coord spin: roster down: {e}")),
        }
        // its process is gone once roster's hang-up returns; coordd takes that from its own watch of it
        while shared.lock().await.agents.get(&name).is_some_and(|a| a.running()) {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
    if how != SpinEnd::Close {
        let r = shared.lock().await.set_ended(&Who::Owner, &name, false);
        if !r.ok {
            return r;
        }
    }
    let done = match how {
        SpinEnd::Join => format!("{name}: joined back into {from}"),
        SpinEnd::Close => format!("{name}: closed"),
        SpinEnd::Delete => format!("{name}: deleted"),
    };
    eprintln!("coordd: the owner's {done}");
    ok(done)
}

/// `Request::RawPage` (N-802): the raw pane's lines of agent NAME's requests, for a window that cannot reach the
/// proxy's log (Windows). Where they are (`record_source`: its session's own record, when no gateway's log holds them;
/// else `proxy_source`: the agent, the caller above it, its session id) is taken under the lock; the folder, resolved
/// with coordd's own home, is read off it on a blocking thread (a page runs to a megabyte), bodies only (the proxylog
/// crate). A coordd whose home does not hold the folder (a boxed one) says so.
async fn raw_page(shared: &Shared, w: &Who, name: String, before: Option<u64>, after: Option<u64>, span: Option<(u64, u64)>) -> Reply {
    let (src, file) = {
        let core = shared.lock().await;
        (core.record_source(w, &name).ok_or_else(|| core.proxy_source(w, &name)), core.raw_file(w, &name))
    };
    let (kind, file) = match file {
        Ok(f) => f,
        Err(e) => return err(e),
    };
    let src = match src {
        // no gateway's log holds its requests: its session's own record, said so (fix 5)
        Ok((file, about)) => return own_record_page(file, about, before, after, span).await,
        Err(s) => s,
    };
    // why the log named for it cannot be read here; none when no log is ours to read (its kind, or no session yet)
    let place = match src {
        Ok(s) => match proxylog::host_dir(&s.dir, &std::env::var("HOME").unwrap_or_default()) {
            Ok(dir) if std::path::Path::new(&dir).is_dir() => return proxy_page(dir, s.session, before, after, span).await,
            Ok(_) => Some("coord raw: the proxy's log cannot be read where this project's server runs".to_string()),
            Err(e) => Some(format!("coord raw: {e}")),
        },
        Err(_) => None,
    };
    match (kind.as_str(), file, place) {
        ("claude", Some(f), _) => own_record_page(f, raw_file::LOG_NOT_HERE.into(), before, after, span).await,
        (_, Some(f), _) => match tokio::task::spawn_blocking(move || raw_file::page(&f, before, after, span)).await {
            Ok(page) => ok(serde_json::to_string(&page).unwrap()),
            Err(e) => err(format!("coord raw: {e}")),
        },
        (_, None, Some(why)) => err(why),
        // no session file yet: an empty page, which the pane says as no turns recorded
        (_, None, None) => ok(serde_json::to_string(&proxylog::RawPage::default()).unwrap()),
    }
}

/// [`raw_page`] from the proxy's log folder DIR, of SESSION, read off the lock on a blocking thread.
async fn proxy_page(dir: String, session: String, before: Option<u64>, after: Option<u64>, span: Option<(u64, u64)>) -> Reply {
    let done = tokio::task::spawn_blocking(move || match span {
        Some((since, until)) => proxylog::turn_page(&dir, &session, since, until),
        None => proxylog::page(&dir, &session, before, after),
    })
    .await;
    match done {
        Ok(page) => ok(serde_json::to_string(&page).unwrap()),
        Err(e) => err(format!("coord raw: {e}")),
    }
}

/// [`raw_page`] of an agent no gateway's log holds the requests of, from its session FILE's own record
/// (`transcript::record_page`, off the lock on a blocking thread), ABOUT said over it (fix 5, accounts-lead N-1446
/// p2446).
async fn own_record_page(file: String, about: String, before: Option<u64>, after: Option<u64>, span: Option<(u64, u64)>) -> Reply {
    let done = tokio::task::spawn_blocking(move || {
        let page = match span {
            Some((since, until)) => transcript::record_turn_page(&file, since, until),
            None => transcript::record_page(&file, before, after),
        };
        serde_json::to_string(&proxylog::RawPage { about: Some(about), ..page }).unwrap()
    })
    .await;
    match done {
        Ok(page) => ok(page),
        Err(e) => err(format!("coord raw: {e}")),
    }
}

/// How many transcript pages are read at once, off the lock (`transcript_page`): a window's reconnect asks one of each
/// agent's sessions together (60 to 123 in a minute on research), each read on a blocking thread of its own, each its
/// own arena (N-1446 p2305); a few at a time keep the threads, and the windows in memory at once, few.
const PAGES_AT_ONCE: usize = 4;
static PAGES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(PAGES_AT_ONCE);

/// `Request::Transcript`: a page of agent NAME's session for the app's transcript view (`transcript::page`), its
/// current one or SESSION, another of its transcripts. Which session, whether W may read it, and his system-prompt
/// edits in it are taken under the lock (`Core::transcript_of`); the window (up to 32 MB) read and parsed, and the
/// reply written, off it on a blocking thread, PAGES_AT_ONCE at a time (paper-research N-1446 p2313: a reconnect's
/// pages held research's lock 0.9 to 2.2 s, every delivery waiting).
async fn transcript_page(shared: &Shared, w: &Who, name: String, before: Option<u64>, after: Option<u64>, session: Option<String>) -> Reply {
    let found = match shared.lock().await.transcript_of(w, &name, session.as_deref()) {
        Ok(s) => s,
        Err(e) => return err(e),
    };
    let Some((session, restarted)) = found else { return ok(serde_json::to_string(&coord_types::Transcript::default()).unwrap()) };
    let _turn = PAGES.acquire().await;
    let done = tokio::task::spawn_blocking(move || {
        let page = coord_types::Transcript { session: session.clone(), restarted, ..transcript::page(&session, before, after) };
        serde_json::to_string(&page).unwrap()
    })
    .await;
    match done {
        Ok(page) => ok(page),
        Err(e) => err(format!("coord transcript: {e}")),
    }
}

/// The owner's file requests (`files`), or None for any other request. The coord's folders are taken under the lock;
/// the disk is read and written off it, on a blocking thread.
async fn owner_files(shared: &Shared, w: &Who, req: &Request) -> Option<Reply> {
    let what = match req {
        Request::Roots => "roots",
        Request::ListDir { .. } => "listdir",
        Request::ReadFile { .. } => "readfile",
        Request::Locate { .. } => "locate",
        Request::Sources => "sources",
        Request::EditFile { .. } => "editfile",
        Request::PdfPage { .. } => "pdfpage",
        _ => return None,
    };
    if !matches!(w, Who::Owner) {
        return Some(err(format!("coord {what}: the owner's (the app's Files page and edits)")));
    }
    let at = shared.lock().await.file_places();
    let req = req.clone();
    let done = tokio::task::spawn_blocking(move || match &req {
        Request::Roots => Ok(serde_json::to_string(&files::roots(&at)).unwrap()),
        Request::ListDir { path, git } => files::list_dir(&at, path, *git),
        Request::ReadFile { path, from, len } => files::read_file(&at, path, from.unwrap_or(0), *len),
        Request::Locate { path, cwd, search } => files::locate(&at, path, cwd.as_deref(), search),
        Request::Sources => Ok(files::sources(&at)),
        Request::PdfPage { path, page, width } => files::pdf_page(&at, path, *page, *width),
        Request::EditFile { path, old, text, message } => {
            let r = files::edit_file(&at, path, old.as_deref(), text, message);
            eprintln!("coordd: the owner's edit of {path}: {}", r.as_ref().unwrap_or_else(|e| e));
            r
        }
        _ => unreachable!(),
    })
    .await;
    Some(match done {
        Ok(Ok(out)) => ok(out),
        // an edit's refusal is the owner's to read under his Save, already in his words (save_words, N-802)
        Ok(Err(why)) if what == "editfile" => err(why),
        Ok(Err(why)) => err(format!("coord {what}: {why}")),
        Err(e) => err(format!("coord {what}: {e}")),
    })
}

/// `Request::AddRepo` (fix 3, the project's Home; ADD_REPO_PROTOCOL): the owner's only. The coord's places are taken
/// under the lock, the project file written off it (project.rs `add_repo`), then read again under it and a snapshot
/// sent to every window, as a text change's views are (change.rs): the window's Home lists the repository, and
/// `places.repo` and `places.start` say where New agent starts now.
async fn add_repo(shared: &Shared, w: &Who, name: String, path: String) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord addrepo: the owner's (his project's Home)".into());
    }
    let at = shared.lock().await.file_places();
    let said = match tokio::task::spawn_blocking(move || project::add_repo(&at, &name, &path)).await {
        Ok(Ok(said)) => said,
        Ok(Err(why)) => return err(why),
        Err(e) => return err(format!("coord addrepo: {e}")),
    };
    eprintln!("coordd: the owner's Add a repository: {said}");
    let mut core = shared.lock().await;
    match project::Project::load(&core.coord) {
        Ok(p) => core.project = p,
        Err(e) => return err(format!("{said}; but the project file does not load again: {e}")),
    }
    core.resolve_modes();
    let snap = core.snapshot();
    core.emit(snap.body);
    ok(said)
}

/// Agent NAME's permission request INPUT (claude/hook): Claude Code's answer, empty where the hook's own refusal stands.
/// A read that waits on the owner holds this request, off the lock, until his answer or `read_ask::WAIT`.
async fn read_permission(shared: &Shared, name: &str, input: &Value) -> String {
    use read_ask::{hook_answer, Judged};
    let judged = shared.lock().await.read_judge(name, input);
    match judged {
        Judged::Pass => String::new(),
        Judged::Allow => hook_answer(Ok(())),
        Judged::Refuse(why) => hook_answer(Err(why)),
        Judged::Wait(_, rx) => match tokio::time::timeout(read_ask::WAIT, rx).await {
            Ok(Ok(answer)) => hook_answer(answer),
            _ => {
                shared.lock().await.changed(name);
                hook_answer(Err(read_ask::UNANSWERED.into()))
            }
        },
    }
}

/// `Request::ReadAnswer`: the owner's. A folder he allows is kept in the project file (`[read.NAME]`) before its calls
/// go through, so the read he allowed is still allowed after a restart; the card goes either way.
async fn read_answer(shared: &Shared, w: &Who, name: String, id: u64, answer: coord_types::ReadAnswer) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord read: an agent's read outside its folders is the owner's to allow".into());
    }
    let (at, folder) = {
        let core = shared.lock().await;
        let Some(ask) = core.read_asks.of(&name).into_iter().find(|k| k.id == id) else { return err(format!("{name} has no read waiting as {id}")) };
        (core.file_places(), ask.folder)
    };
    let mut said = String::new();
    if answer == coord_types::ReadAnswer::Folder {
        let Some(folder) = folder else { return err("that read may be allowed once only: its folder is too wide".into()) };
        let (n, f) = (name.clone(), folder.clone());
        said = match tokio::task::spawn_blocking(move || read_ask::keep(&at, &n, &f)).await {
            Ok(Ok(said)) => said,
            Ok(Err(why)) => return err(why),
            Err(e) => return err(format!("coord read: {e}")),
        };
    }
    let mut core = shared.lock().await;
    if !said.is_empty() {
        match project::Project::load(&core.coord) {
            Ok(p) => core.project = p,
            Err(e) => return err(format!("{said}; but the project file does not load again: {e}")),
        }
    }
    let r = core.read_asks.answer(&name, id, answer);
    core.changed(&name);
    match r {
        Ok((ask, _)) => {
            eprintln!("coordd: the owner's answer to {name}'s read of {}: {answer:?}", ask.path);
            ok(if said.is_empty() { format!("{name}: {answer:?}") } else { said })
        }
        Err(e) => err(e),
    }
}

/// `Request::Settings`: the owner's. Read; or a key set at `agent:NAME` through the agent's own calls, or at `yaaihh` or
/// `project` off the lock (settings.rs), the project read again and every agent's mode taken again, and a snapshot sent.
async fn settings(shared: &Shared, w: &Who, scope: Option<String>, key: Option<String>, value: Option<String>) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord settings: the owner's".into());
    }
    let (Some(scope), Some(key)) = (scope, key) else {
        let core = shared.lock().await;
        return ok(settings::read(&core.coord, core.project.as_ref(), None));
    };
    let value = value.unwrap_or_default();
    if let Some(name) = scope.strip_prefix("agent:") {
        let mut core = shared.lock().await;
        let r = match (key.as_str(), value.as_str()) {
            ("agent_mode", v) => core.mode(w, name, v),
            ("safety_check", "") => core.clear_switch(w, name, "safety_check"),
            ("safety_check", v @ ("true" | "false")) => core.harness_switches(w, name, Some(v == "true"), None, None),
            (k, v) => err(format!("coord settings: {k} = {v} is not an agent's setting")),
        };
        if !r.ok {
            return r;
        }
        let snap = core.snapshot();
        core.emit(snap.body);
        return ok(settings::read(&core.coord, core.project.as_ref(), Some(&r.out)));
    }
    let (at, project) = {
        let core = shared.lock().await;
        (core.file_places(), core.project.clone())
    };
    let said = match tokio::task::spawn_blocking(move || settings::set(&at, project.as_ref(), &scope, &key, &value)).await {
        Ok(Ok(said)) => said,
        Ok(Err(why)) => return err(why),
        Err(e) => return err(format!("coord settings: {e}")),
    };
    eprintln!("coordd: the owner's setting: {said}");
    let mut core = shared.lock().await;
    match project::Project::load(&core.coord) {
        Ok(p) => core.project = p,
        Err(e) => return err(format!("{said}; but the project file does not load again: {e}")),
    }
    core.resolve_modes();
    core.check_taken(None);
    let snap = core.snapshot();
    core.emit(snap.body);
    ok(settings::read(&core.coord, core.project.as_ref(), Some(&said)))
}

/// `Request::RepoState`: the owner's; git asked off the lock for each repository of the project.
async fn repo_state(shared: &Shared, w: &Who) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord repostate: the owner's".into());
    }
    let repos = shared.lock().await.project.as_ref().map(|p| p.repos()).unwrap_or_default();
    let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
    match tokio::task::spawn_blocking(move || machine_state::repos::all(&repos, &home)).await {
        Ok(out) => ok(out.to_string()),
        Err(e) => err(format!("coord repostate: {e}")),
    }
}

/// `Request::CodeView`: the owner's; the roots taken under the lock, the server and the open off it.
async fn code_view(shared: &Shared, w: &Who, target: coord_types::codeview::CodeTarget, theme: Option<String>, looks: Option<coord_types::codeview::CodeLooks>) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord codeview: the owner's".into());
    }
    let ctx = code_ctx(&*shared.lock().await);
    match tokio::task::spawn_blocking(move || codeview::open(&ctx, &target, theme.as_deref(), looks.as_ref())).await {
        Ok(Ok(a)) => ok(serde_json::to_string(&a).unwrap_or_default()),
        Ok(Err(why)) => err(why),
        Err(e) => err(format!("coord codeview: {e}")),
    }
}

/// What the code view needs of CORE: its folder, HOME, agent-base and the project's Files roots that are folders here.
fn code_ctx(core: &Core) -> codeview::Ctx {
    codeview::Ctx {
        coord: core.coord.clone().into(),
        home: std::env::var("HOME").unwrap_or_default(),
        agent_base: core.base.clone().into(),
        roots: files::roots(&core.file_places()).into_iter().filter(|r| r.here).map(|r| (r.name, r.path)).collect(),
    }
}

/// `coord start` (N-101 post 406, the joint proposal's A): the owner's `roster add NAME MODEL PROMPT DIR`, or `roster
/// restart NAME` (no MODEL), of an agent of a boxed project, which roster hands here (`$COORD/boxed`) so that the agent
/// is started as coordd starts every agent of the project: `launch`, its own user. Registered first, as roster does.
async fn start(shared: &Shared, w: &Who, name: String, model: Option<String>, prompt: Option<String>, dir: Option<String>, force: bool) -> Reply {
    if !matches!(w, Who::Owner) {
        return err("coord start: the owner's, through roster (an agent makes agents with coord new or coord fork)".into());
    }
    let (model, dir) = {
        let mut core = shared.lock().await;
        let Some(repo) = core.project.as_ref().filter(|p| p.project.boxed).map(|p| p.repo().unwrap_or_default()) else {
            return err(format!("coord start {name}: the project is not boxed; roster starts it itself"));
        };
        let model = match model {
            Some(m) => {
                let kind = core::kind_of_model(&m);
                let r = core.add(w, &name, kind, &m);
                if !r.ok {
                    return r;
                }
                m
            }
            None => match core.agents.get(&name) {
                Some(a) => a.model.clone(),
                None => return err(format!("coord start {name}: no such agent (roster add starts a new one)")),
            },
        };
        (model, dir.unwrap_or(repo))
    };
    // a step's agent that owes a step in a layer starts in it, so its turn can answer the step (N-101 p1778, 3); one
    // that owes steps in more than one layer is refused, each named: the first would be a choice nobody made (p1793)
    let owed: Vec<(String, String)> = {
        let core = shared.lock().await;
        let m = &core.tickets.m;
        let active: Vec<String> = m.active_layers().into_iter().map(|(l, _, _)| l).collect();
        let mut owed = m.owes(&name);
        owed.sort();
        owed.into_iter().filter_map(|(t, _, _)| m.layer_of(t).filter(|l| active.contains(l)).map(|l| (m.id(t), l))).collect()
    };
    if let Err(e) = one_layer(&name, &owed) {
        return err(e);
    }
    let (layer, what) = match shared.lock().await.start_layer(&name) {
        Some((t, l)) => {
            let id = shared.lock().await.tickets.m.id(t);
            (Some(l), format!("for the owner (roster), in the layer of its step on {id}"))
        }
        None => (None, String::from("for the owner (roster)")),
    };
    let l = Launch { agent: name.clone(), model, dir, prompt: prompt.unwrap_or_default(), layer, from: None, cut: None, what, force };
    match launch(shared, l).await {
        Ok(()) => ok(format!("{name}: started by coordd")),
        Err(e) => err(format!("coord start {name}: {e}")),
    }
}

/// `coord start NAME` with OWED, the tickets of the steps NAME owes and the open layer of each: refused when they are
/// in more than one layer, naming each.
fn one_layer(name: &str, owed: &[(String, String)]) -> Result<(), String> {
    let mut layers: Vec<&str> = owed.iter().map(|(_, l)| l.as_str()).collect();
    layers.sort();
    layers.dedup();
    if layers.len() <= 1 {
        return Ok(());
    }
    let each: Vec<String> = owed.iter().map(|(t, l)| format!("{t} in {l}")).collect();
    Err(format!("coord start {name}: it owes steps in {} layers ({}), so there is no one place to start it; nothing started", layers.len(), each.join("; ")))
}

/// The limits written at coordd's start (paper-research N-101 p1785): each `[limits.NAME]`, and every agent group there
/// now its own over the box's (`box_memory`, `box_cpu`, `box_pids`) for a boxed one, so a limit added to the project
/// file holds from this start for agents running already; a program agent (the runner) has its own only.
fn start_limits(core: &Core) -> Vec<(String, sandbox::Limits)> {
    let Some(p) = core.project.as_ref() else { return Vec::new() };
    let mut names: std::collections::BTreeSet<String> = p.limits.keys().cloned().collect();
    for e in std::fs::read_dir(cgroup::root().join("agents")).into_iter().flatten().flatten().filter(|e| e.path().is_dir()) {
        names.insert(e.file_name().to_string_lossy().into_owned());
    }
    names
        .into_iter()
        .map(|n| {
            let boxed = p.project.boxed && core.agents.get(&n).is_some_and(|a| a.kind != "program");
            let l = p.agent_limits(&n, boxed);
            (n, l)
        })
        .filter(|(_, l)| *l != sandbox::Limits::default())
        .collect()
}

/// An agent to start (`launch`): AGENT on MODEL, in DIR unless LAYER's view puts it elsewhere, its first words the
/// file PROMPT (after roster's fork note when it is a fork of FROM); WHAT, for coordd's log.
pub(crate) struct Launch {
    pub agent: String,
    pub model: String,
    pub dir: String,
    pub prompt: String,
    pub layer: Option<String>,
    pub from: Option<String>,
    /// A copy of FROM (N-1104 p1154): its session cut after the line this delivery was given on (`Core::fork_place`,
    /// follow.rs `fork_cut`), not as it stands; roster makes the cut copy beside FROM's transcript (`fork --cut`).
    pub cut: Option<String>,
    pub what: String,
    /// `roster restart NAME --force`: restarted though coordd holds a turn running (the owner's `coord start --force`)
    pub force: bool,
}

/// Start L as the project starts its agents: in its layer's view and, where the project is boxed, as a user of its
/// own with the project's box limits, read-only folders and box_setup (sandbox.rs); a fork with FROM's conversation
/// copied where its session finds it; one that ran before on its own conversation. Why it was not started, otherwise.
/// A step's agent (`start_agent`) and an agent made from an agent's turn (make.rs) both start here.
pub(crate) async fn launch(shared: &Shared, l: Launch) -> Result<(), String> {
    let Launch { agent, model, dir, prompt, layer, from, cut, what, force } = l;
    // a boxed project never starts an agent outside its sandbox (the joint proposal's I): outside a step's layer it
    // runs in the line's read-only view (B; docket-core2, N-229 p5), boxed there as in a layer
    let layer = match layer {
        None if shared.lock().await.project.as_ref().is_some_and(|p| p.project.boxed) => Some(moves::LINE.to_string()),
        l => l,
    };
    let (base, spec, boxed, source, cut) = {
        let core = shared.lock().await;
        if let Some(at) = core.agents.get(&agent).and_then(|a| a.ended.as_ref()) {
            return Err(format!("{agent} was ended by the owner at {at}"));
        }
        // the line's read-only view (`moves::LINE`, a `line@` id): made from the line's spec (N-101 p406, B)
        let spec = layer.as_ref().map(|l| if layer::is_line(l) { core.line_spec() } else { core.layer_spec(l) });
        // the folders of the project's shared language servers' sockets (`[[lsp]] socket`), bound into a box: of the
        // coord's folder a box sees only its own parts (box-enter, paper-research N-101 p573), and a server started
        // again makes a new socket in the same folder, which a bound file would not show
        let lsp_dirs: Vec<String> = core.project.as_ref().map(|p| p.lsps.iter().filter_map(|l| l.socket.as_deref()).map(|s| project::Lsp::expand(s, &core.places().coord)).filter_map(|s| std::path::Path::new(&s).parent().map(|d| d.display().to_string())).collect()).unwrap_or_default();
        let boxed = core.project.as_ref().filter(|p| p.project.boxed).map(|p| (p.agent_read(&agent), p.agent_limits(&agent, true), format!("{}/claude-settings.json", p.dir), p.box_setup(), lsp_dirs));
        // a fork: the session it copies and that session's transcript, as they stand now (N-101)
        let source = match from.as_deref() {
            None => None,
            Some(f) => {
                let a = core.agents.get(f);
                let sid = a.and_then(|a| a.session.get("session_id").and_then(Value::as_str).map(str::to_string));
                match (sid, a.and_then(|a| a.transcript())) {
                    (Some(sid), Some(file)) if a.is_some_and(|a| a.kind == "claude") => Some((f.to_string(), sid, file)),
                    _ => return Err(format!("{f} has no Claude Code session to fork")),
                }
            }
        };
        // a copy: where its session is cut, the bytes of FROM's transcript up to the entry that took CUT's message,
        // as part 4's restart reads it (`fork_now`); outside a box and a view only, as that restart is
        let cut = match (cut.as_deref(), &source) {
            (None, _) => None,
            (Some(place), Some((f, _, file))) => {
                if spec.is_some() {
                    return Err(format!("a copy of {f} is not cut in a view or a box yet"));
                }
                let w = core.fork_place(f, place)?;
                if &w.file != file {
                    return Err(format!("{place} was given in {}, not in the session {f} runs", w.file));
                }
                let upto = crate::follow::fork_cut(&w.file, w.offset, place).ok_or_else(|| format!("no entry of {} after {place}'s given line holds its message", w.file))?;
                Some((w.file, upto))
            }
            (Some(_), None) => return Err(String::from("a copy names no agent it copies")),
        };
        (core.base.clone(), spec, boxed, source, cut)
    };
    // its first prompt, so that the session's first user entry is not taken for words typed into its terminal
    // (`Core::typed`): the prompt file's text, without the `resume:` line roster reads first
    if let Ok(text) = std::fs::read_to_string(&prompt) {
        let text = if text.starts_with("resume:") { text.split_once('\n').map_or("", |(_, r)| r).to_string() } else { text };
        shared.lock().await.launched_with.insert(agent.clone(), text);
    }
    // in a layer: started inside its view, at the project's path (the view made first if it is not up)
    let mut roster = tokio::process::Command::new(format!("{base}/bin/roster"));
    // started by coordd: roster does not hand it back here (`coord start`)
    roster.env("ROSTER_VIA_COORDD", "1");
    let mut dir = dir;
    let mut inside = String::new();
    // boxed: its inside uid (the owner of a fork's copy)
    let mut uid = None;
    let layered = spec.is_some();
    if let Some(spec) = spec {
        let coord = shared.lock().await.places().coord.clone();
        let line = layer.as_deref().is_some_and(layer::is_line);
        let opened = match spec {
            Ok(spec) if line => tokio::task::spawn_blocking(move || layer::line_view(&coord, &spec, &core::now())).await.unwrap_or_else(|e| Err(e.to_string())),
            Ok(spec) => tokio::task::spawn_blocking(move || layer::open(&coord, &spec, &core::now())).await.unwrap_or_else(|e| Err(e.to_string())),
            Err(e) => Err(e),
        };
        match opened {
            Ok(l) => {
                // at the view's path of the project's folder, whatever folder it was registered in: a restart too
                // (roster's `restart` takes ROSTER_DIR over its session's cwd), else a boxed agent registered in a
                // folder above the repositories had that whole folder bound into its box, the repositories' siblings
                // with it (docket-core3, N-101 p536)
                dir = l.repo.clone();
                roster.env("ROSTER_DIR", &dir);
                // the view's other repositories, each bound into the box at its own path (box-enter): a boxed agent
                // sees only what is bound in, and DIR is the project's own repository
                roster.env("ROSTER_BOX_ALSO", l.also.iter().map(|p| p.repo.as_str()).collect::<Vec<_>>().join(":"));
                // marked as an agent of a layer: never resumed outside it (Core::hand_over_running)
                let a = format!("{}/agents/{agent}", shared.lock().await.places().coord);
                if let Err(e) = std::fs::create_dir_all(&a).and_then(|_| std::fs::write(format!("{a}/layer"), format!("{}\n", l.id))) {
                    return Err(format!("marking it an agent of layer {}: {e}", l.id));
                }
                roster.env("ROSTER_VIEW", l.holder.unwrap_or_default().to_string());
                // the layer, for the symbols server's shared language servers (symbols lsp.rs client_options)
                roster.env("COORD_LAYER", &l.id);
                inside = format!(", in layer {}", l.id);
                // boxed (sandbox.rs): its own uid, home and limits; roster's open enters the box
                if let (true, Some((read, limits, psettings, setup, lsp_dirs))) = (l.boxed, &boxed) {
                    let coord = shared.lock().await.places().coord.clone();
                    // its own uid, its limits on agents/NAME and its home, as at its registration (sandbox::ready)
                    let (c2, a2, d2, b2, ps, su, li) = (coord.clone(), agent.clone(), l.repo.clone(), base.clone(), psettings.clone(), setup.clone(), limits.clone());
                    let built = tokio::task::spawn_blocking(move || sandbox::ready(&sandbox::Home { coord: &c2, name: &a2, k: 0, dir: &d2, base: &b2, project_settings: Some(ps), owner: format!("{}/.claude.json", std::env::var("HOME").unwrap_or_default()), setup: su }, &li)).await.unwrap_or_else(|e| Err(e.to_string()));
                    let k = match built {
                        Ok(k) => k,
                        Err(e) => {
                            shared.lock().await.boxed_or_not(&agent, Some(e.clone()));
                            return Err(format!("its box: {e}"));
                        }
                    };
                    shared.lock().await.boxed_or_not(&agent, None);
                    roster.env("ROSTER_BOX", k.to_string()).env("ROSTER_BOX_DIR", format!("{coord}/box/{agent}")).env("ROSTER_BOX_READ", read.join(":")).env("ROSTER_BOX_SOCKETS", lsp_dirs.join(":"));
                    uid = Some(k);
                    inside = format!("{inside}, boxed as uid {}", sandbox::host_uid(k).unwrap_or_default());
                }
            }
            Err(e) => {
                // a boxed project's agent with no view to run in is not started: why, on the agent (coord ls)
                if line && boxed.is_some() {
                    shared.lock().await.boxed_or_not(&agent, Some(format!("the line's view: {e}")));
                }
                return Err(format!("its layer: {e}"));
            }
        }
    }
    // an agent that ran before (its session kept, agents/NAME/session.json) is started again on its own
    // conversation (N-101: a step to an agent whose block ended), in this step's layer and box
    let coord = shared.lock().await.places().coord.clone();
    let session = format!("{coord}/agents/{agent}/session.json");
    let mut again = std::path::Path::new(&session).is_file();
    // a Claude Code session whose transcript was never written (its folder refused it: Tovrin-dev's orchestrator,
    // N-101 p1223) cannot be resumed: `claude --resume` exits 1 with "No conversation found". Its session.json is set
    // aside and the agent starts afresh. Only a transcript known to be gone counts: one coordd may not read is kept
    if again {
        let gone = {
            let core = shared.lock().await;
            core.agents.get(&agent).filter(|a| a.kind == "claude").is_some_and(|a| match a.transcript() {
                None => true,
                Some(t) => std::fs::metadata(&t).err().is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            })
        };
        if gone {
            let aside = format!("{session}.no-transcript");
            match std::fs::rename(&session, &aside) {
                Ok(()) => {
                    eprintln!("coordd: tickets: {agent}'s session has no transcript to resume: started afresh (its session.json kept as {aside})");
                    again = false;
                }
                Err(e) => return Err(format!("its session has no transcript to resume, and setting it aside failed: {e}")),
            }
        }
    }
    // the folder its Claude Code runs with outside a view (fork::claude_dir): the project's own claude-home, unless it
    // is on one of the owner's accounts
    let claude = {
        let (store, coord_id) = {
            let core = shared.lock().await;
            (core.accounts.clone(), core.coord_id())
        };
        let on_account = store.bound_to(accounts::Harness::Claude, &coord_id, &agent).is_some();
        fork::claude_dir(&coord, &std::env::var("HOME").unwrap_or_default(), on_account)
    };
    let (how, out) = if again {
        // its conversation where its session looks for it in this place (a view's conversations folder, or the
        // project's or the owner's projects folder outside one): copied there when it was written elsewhere (a move,
        // moves.rs)
        let found = {
            let core = shared.lock().await;
            core.agents.get(&agent).filter(|a| a.kind == "claude").and_then(|a| Some((a.session.get("session_id").and_then(Value::as_str)?.to_string(), a.transcript()?)))
        };
        if let Some((sid, file)) = found.filter(|(_, f)| std::path::Path::new(f).is_file()) {
            let to = fork::place(&dir, layered, &claude, &sid);
            let (file2, to2) = (file.clone(), to.clone());
            match tokio::task::spawn_blocking(move || fork::copy(&file2, &to2, uid)).await.unwrap_or_else(|e| Err(e.to_string())) {
                Ok(true) => inside = format!("{inside}, its conversation copied to {to}"),
                Ok(false) => {}
                Err(e) => return Err(format!("its conversation: {e}")),
            }
        }
        ("restart", roster.args(["restart", &agent]).args(force.then_some("--force")).output().await)
    } else if let (Some((f, sid, _)), Some((file, upto))) = (&source, &cut) {
        // a copy (N-1104 p1154): roster cuts FROM's transcript beside it, a session of its own, and forks that, in
        // FROM's own folder (`start_agent`), where the resume finds it; its first prompt roster's fork note, then
        // the copy's prompt (agent-base prompts/copy.md)
        let a = format!("{coord}/agents/{agent}");
        if let Err(e) = std::fs::create_dir_all(&a).and_then(|_| std::fs::write(format!("{a}/fork"), format!("{f} {sid} {upto}\n"))) {
            return Err(format!("marking it a copy of {f}: {e}"));
        }
        inside = format!("{inside}, a copy of {f} (session {sid} cut after {upto} bytes)");
        let text = std::fs::read_to_string(&prompt).unwrap_or_default();
        ("fork", roster.args(["fork", f, "--cut", file, &upto.to_string(), "--name", &agent, "--model", &model, "--dir", &dir, text.trim_end()]).output().await)
    } else if let Some((f, sid, file)) = &source {
        // a fork (N-101, fork.rs): FROM's transcript copied where the new session is found, then `roster fork`
        // in this step's view and box; its first prompt roster's fork note, then the call's prompt
        let to = fork::place(&dir, layered, &claude, sid);
        let (file2, to2) = (file.clone(), to.clone());
        match tokio::task::spawn_blocking(move || fork::copy(&file2, &to2, uid)).await.unwrap_or_else(|e| Err(e.to_string())) {
            Ok(copied) => {
                let a = format!("{coord}/agents/{agent}");
                if let Err(e) = std::fs::create_dir_all(&a).and_then(|_| std::fs::write(format!("{a}/fork"), format!("{f} {sid}\n"))) {
                    return Err(format!("marking it a fork of {f}: {e}"));
                }
                inside = format!("{inside}, a fork of {f} (session {sid}{})", if copied { format!(", copied to {to}") } else { String::new() });
            }
            Err(e) => return Err(format!("forking {f}: {e}")),
        }
        let text = std::fs::read_to_string(&prompt).unwrap_or_default();
        ("fork", roster.args(["fork", f, "--name", &agent, "--model", &model, "--dir", &dir, text.trim_end()]).output().await)
    } else {
        ("add", roster.args(["add", &agent, &model, &prompt, &dir]).output().await)
    };
    match out {
        Ok(o) if o.status.success() => {
            if again {
                eprintln!("coordd: tickets: {agent} started again on its conversation {what}{inside}");
            } else {
                eprintln!("coordd: tickets: {agent} started {what} ({model}, in {dir}{inside})");
            }
            Ok(())
        }
        Ok(o) => Err(format!("roster {how} failed: {}", String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => Err(format!("roster {how}: {e}")),
    }
}

/// A fresh agent's prompt (paper-research m971, 11): `prompts/CALL.md` beside a calls folder of CALL_DIRS (the
/// project's first), else BASE's neutral `prompts/fresh.md`.
fn fresh_prompt(call_dirs: &[String], base: &str, call: &str) -> String {
    call_dirs
        .iter()
        .rev()
        .map(|d| format!("{}/prompts/{call}.md", std::path::Path::new(d).parent().map(|p| p.display().to_string()).unwrap_or_default()))
        .find(|p| std::path::Path::new(p).is_file())
        .unwrap_or_else(|| format!("{base}/prompts/fresh.md"))
}

#[cfg(test)]
mod fresh_tests {
    use super::*;

    /// A typed post or msg, come as its alias, is timed as its variant (proto-reader N-1481 p61).
    #[test]
    fn a_typed_post_is_timed_as_a_post() {
        let req = |l: &str| serde_json::from_str::<Request>(l).unwrap();
        assert_eq!(timed_as(&req(r#"{"op":"typedpost","t":"N-12","text":"x","kind":{"is":"fyi"}}"#)), "post");
        assert_eq!(timed_as(&req(r#"{"op":"post","t":"N-12","text":"x"}"#)), "post");
        assert_eq!(timed_as(&req(r#"{"op":"typedmsg","to":[],"body":"x","re":"N-12","kind":{"is":"fyi"}}"#)), "msg");
    }

    /// N-39, 2: a Codex agent's levels are its model's in Codex's catalog, as `codex debug models` prints it.
    #[test]
    fn a_codex_models_efforts_are_its_catalogs() {
        let cat = br#"{"models":[{"slug":"gpt-a","supported_reasoning_levels":[{"effort":"low","description":"x"},{"effort":"ultra","description":"y"}]},{"slug":"gpt-b","supported_reasoning_levels":[]}]}"#;
        assert_eq!(codex_model_efforts(cat, "gpt-a"), Ok(vec!["low".to_string(), "ultra".to_string()]));
        assert_eq!(codex_model_efforts(cat, "gpt-c"), Err("Codex's catalog has no model gpt-c".to_string()));
        assert_eq!(codex_model_efforts(cat, "gpt-b"), Err("Codex's catalog names no levels for gpt-b".to_string()));
    }

    #[test]
    fn a_fresh_agent_runs_where_the_work_is() {
        let cwds = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let files = Some("/c/files");
        assert_eq!(fresh_dir(&json!({"repo": "/r"}), None, &cwds(&["/w"]), files).as_deref(), Some("/r"));
        assert_eq!(fresh_dir(&json!({}), None, &cwds(&["/w", "/c/files", "/c/files", "/w", "/x"]), files).as_deref(), Some("/w"));
        assert_eq!(fresh_dir(&json!({"repo": "/c/files/"}), None, &cwds(&["/c/files"]), files), None);
        // the project's folder when its file sets one (N-89.p4); a ticket's own repo still first
        assert_eq!(fresh_dir(&json!({}), Some("/p"), &cwds(&["/w"]), files).as_deref(), Some("/p"));
        assert_eq!(fresh_dir(&json!({"repo": "/r"}), Some("/p"), &cwds(&["/w"]), files).as_deref(), Some("/r"));
    }

    #[test]
    fn a_call_brings_its_own_prompt() {
        let d = std::env::temp_dir().join(format!("fresh-prompt-{}", std::process::id()));
        let (base, proj) = (d.join("base"), d.join("proj"));
        for p in [base.join("calls"), base.join("prompts"), proj.join("calls"), proj.join("prompts")] {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(base.join("prompts/fixes.md"), "b").unwrap();
        std::fs::write(proj.join("prompts/better.md"), "p").unwrap();
        let dirs = vec![base.join("calls").display().to_string(), proj.join("calls").display().to_string()];
        let b = base.display().to_string();
        assert_eq!(fresh_prompt(&dirs, &b, "fixes"), format!("{b}/prompts/fixes.md"));
        assert_eq!(fresh_prompt(&dirs, &b, "better"), format!("{}/prompts/better.md", proj.display()));
        assert_eq!(fresh_prompt(&dirs, &b, "outline"), format!("{b}/prompts/fresh.md"));
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod base_link_tests {
    /// A session's AGENT_BASE is the coord's link: a release's coordd started over another's turns it to its own
    /// agent-base, so the client a running session calls is the new release's (yaaihh N-1804 p2); a folder there is
    /// left alone.
    #[test]
    fn the_coords_agent_base_follows_the_coordd_that_started_last() {
        let d = std::env::temp_dir().join(format!("coordd-base-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (coord, old, new) = (d.join("coord"), d.join("rel-1/agent-base"), d.join("rel-2/agent-base"));
        for f in [&coord, &old.join("bin"), &new.join("bin")] {
            std::fs::create_dir_all(f).unwrap();
        }
        std::fs::write(old.join("bin/coordc"), "old").unwrap();
        std::fs::write(new.join("bin/coordc"), "new").unwrap();
        let link = super::point_base(&coord, &old).unwrap();
        assert_eq!(link, coord.join("agent-base"));
        let client = link.join("bin/coordc");
        assert_eq!(std::fs::read_to_string(&client).unwrap(), "old");
        assert_eq!(super::point_base(&coord, &old).unwrap(), link, "again: as it was");
        super::point_base(&coord, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&client).unwrap(), "new", "the same path, the coordd started last's client");
        assert!(std::fs::read_dir(&coord).unwrap().all(|e| e.unwrap().file_name() == "agent-base"), "no new link left over");
        let other = d.join("coord2");
        std::fs::create_dir_all(other.join("agent-base")).unwrap();
        assert!(super::point_base(&other, &new).is_err(), "a folder there is not replaced");
        assert!(other.join("agent-base").is_dir());
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod start_tests {
    /// The owner's `coord start NAME` (paper-research N-101 p1793): steps owed in one layer start it there; in more
    /// than one, refused with each ticket and layer named; none, the line's view as before.
    #[test]
    fn a_start_owing_steps_in_two_layers_is_refused_naming_each() {
        let s = |v: &[(&str, &str)]| v.iter().map(|(t, l)| (t.to_string(), l.to_string())).collect::<Vec<_>>();
        assert_eq!(super::one_layer("f1729-1", &[]), Ok(()));
        assert_eq!(super::one_layer("f1729-1", &s(&[("P-1729", "P-1729.l1"), ("P-1730", "P-1729.l1")])), Ok(()), "two steps, one layer");
        assert_eq!(
            super::one_layer("f1729-1", &s(&[("P-1724", "P-1724.l1"), ("P-1729", "P-1729.l1")])),
            Err("coord start f1729-1: it owes steps in 2 layers (P-1724 in P-1724.l1; P-1729 in P-1729.l1), so there is no one place to start it; nothing started".into())
        );
    }
}
