//! The daemon: loads the texts given, links the harness drivers, and serves the record on one socket.

use coordd_v2::docket::Docket;
use coordd_v2::hand::Hand;
use coordd_v2::hooks::{Held, Hooks};
use coordd_v2::link::{link, linked, wait_on, Sessions, Waits};
use coordd_v2::turns::watched;
use coordd_v2::peer::{peer_pid, vouched, written, Party, Table};
use coordd_v2::serve::{Engine, Terminals};
use coordd_v2::start::{profiles, session, Places};
use docket_hooks::{Frame, Native};
use docket_v2::run::driver::Driver;
use docket_v2::run::machine::Machine;
use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{broadcast, mpsc};

const USAGE: &str = "coordd-v2 --socket PATH --record PATH --parties PATH --top MODULE [--by-socket PATH] [--compute PATH] [--window PATH] [--turns PATH] [--host PATH] [--clerk PATH] [--record-place PATH] [--relay PATH --profiles PATH --hooks DIR --homes DIR --spawn PATH] TEXT...";

/// How long `coordd-v2 ask` waits for its run's end unless told: a pull or a report runs on facts alone.
const ASK_WAIT: u64 = 30;

/// No driver here holds a terminal yet: every terminal is refused, saying so.
struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, session: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no driver here holds a terminal, so none for {session}"))
    }
}

fn main() {
    // `coordd-v2 act SOCKET NAME ARG...`: a session's own run asked for from its scope (act.rs)
    let a: Vec<String> = std::env::args().collect();
    if a.get(1).map(String::as_str) == Some("act") {
        let [_, _, socket, name, args @ ..] = a.as_slice() else {
            eprintln!("coordd-v2 act SOCKET NAME ARG...");
            std::process::exit(2);
        };
        coordd_v2::act::act(socket.as_ref(), name, args, &mut std::io::stderr(), &coordd_v2::act::alone);
        return;
    }
    // `coordd-v2 ask [--wait SECONDS] SOCKET NAME ARG...`: one Act asked for, its run's value printed (ask.rs)
    if a.get(1).map(String::as_str) == Some("ask") {
        let (wait, rest) = match &a[2..] {
            [w, s, rest @ ..] if w == "--wait" => (s.parse::<u64>().ok(), rest),
            rest => (Some(ASK_WAIT), rest),
        };
        let (Some(wait), [socket, name, args @ ..]) = (wait, rest) else {
            eprintln!("coordd-v2 ask [--wait SECONDS] SOCKET NAME ARG...");
            std::process::exit(2);
        };
        let (words, code) = coordd_v2::ask::told(&coordd_v2::ask::ask(socket.as_ref(), name, args, std::time::Duration::from_secs(wait)));
        if code == 0 { println!("{words}") } else { eprintln!("coordd-v2 ask: {name}: {words}") }
        std::process::exit(code);
    }
    if let Err(e) = run() {
        eprintln!("coordd-v2: {e}");
        std::process::exit(1);
    }
}

/// The flags given, by name, and the texts.
fn args() -> Result<(BTreeMap<String, String>, Vec<PathBuf>), String> {
    let (mut flags, mut texts, mut a) = (BTreeMap::new(), Vec::new(), std::env::args().skip(1));
    while let Some(w) = a.next() {
        match w.strip_prefix("--") {
            Some(f) => drop(flags.insert(f.to_string(), a.next().ok_or(format!("{w} wants a value"))?)),
            None => texts.push(PathBuf::from(w)),
        }
    }
    Ok((flags, texts))
}

fn read(p: &str) -> Result<String, String> {
    std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))
}

/// Harness frame `f`, for the hooks driver and, by its written name, for a session's start.
fn frame<N: Native + 'static>(f: Frame<N>) -> (Rc<RefCell<dyn Held>>, (String, Rc<RefCell<dyn Sessions>>)) {
    let name = written(&f.name());
    let f = Rc::new(RefCell::new(f));
    (f.clone(), (name, f))
}

fn run() -> Result<(), String> {
    let (mut flags, texts) = args()?;
    let mut flag = |f: &str| flags.remove(f);
    let (Some(socket), Some(record), Some(parties), Some(top)) = (flag("socket"), flag("record"), flag("parties"), flag("top")) else { return Err(USAGE.into()) };
    let by_socket = flag("by-socket");
    let compute = flag("compute");
    let turns_file = flag("turns");
    let host = flag("host");
    let clerk = flag("clerk");
    let window = flag("window");
    let record_place = flag("record-place");
    let starts = [flag("relay"), flag("profiles"), flag("hooks"), flag("homes"), flag("spawn")];
    if let Some(f) = flags.keys().next() {
        return Err(format!("--{f}? {USAGE}"));
    }
    let texts = texts.iter().map(|p| read(&p.to_string_lossy()).map(|t| (p.display().to_string(), t))).collect::<Result<Vec<_>, _>>()?;
    let loaded = coordd_v2::texts::load(&texts)?;
    // kept before the `loaded` line that names them is written
    coordd_v2::texts::keep(record.as_ref(), &coordd_v2::codec::hex(&loaded.hash), &texts).map_err(|e| format!("{record}: keeping its texts: {e}"))?;
    // agents' turns read from their transcripts, at the hooks of the harnesses whose transcripts it reads; a copy of a
    // session started by the session driver, linked after it
    let copy = Rc::new(RefCell::new(None));
    let turns = match &turns_file {
        Some(path) => {
            let c: docket_driver_turns::Config = serde_json::from_str(&read(path)?).map_err(|e| format!("{path}: {e}"))?;
            Some(Rc::new(RefCell::new(docket_driver_turns::Turns::new(coordd_v2::turns::daemon(&c, copy.clone()), c)?)))
        }
        None => None,
    };
    let frames_linked = [
        frame(watched(Frame::new(docket_driver_claude::Claude), &turns, docket_driver_claude::HARNESS)),
        frame(watched(Frame::new(docket_driver_codex::Codex), &turns, docket_driver_codex::HARNESS)),
        frame(Frame::new(docket_driver_opencode::Opencode)),
        frame(watched(Frame::new(docket_driver_agy::Agy { version: docket_driver_agy::VERSIONS[0] }), &turns, docket_driver_agy::HARNESS)),
        frame(watched(Frame::new(docket_driver_agy::Agy { version: docket_driver_agy::VERSIONS[1] }), &turns, docket_driver_agy::HARNESS)),
    ];
    // the frames share the hook Effects, so one driver carries them, each line to its agent's frame
    let (held, frames): (Vec<_>, Vec<_>) = frames_linked.into_iter().unzip();
    let (d, w) = link(Hooks(held));
    let hooks_name = written(&d.name());
    let (mut drivers, mut waits): (Vec<Box<dyn Driver>>, Vec<Rc<RefCell<dyn Waits>>>) = (Vec::new(), Vec::new());
    // a started agent's runs are bound to the hooks driver, at its place among the machine's drivers
    let hooks_at = drivers.len();
    drivers.push(d);
    waits.push(w);
    // Command effects as compute jobs, where the file at --compute says they run
    let mut compute_name = None;
    if let Some(path) = compute {
        let c = compute::Config::read(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        let (d, w) = link(docket_driver_compute::Compute::new(c).map_err(|e| e.to_string())?);
        compute_name = Some(written(&d.name()));
        drivers.push(d);
        waits.push(w);
    }
    // his seat, posts and decisions from his window, on the socket the file at --window names; whose each
    // connection is, the table says once it is read
    let mut window_name = None;
    let whose_table = Rc::new(OnceCell::<Rc<Table>>::new());
    if let Some(path) = window {
        let c = docket_driver_window::Config::read(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        let t = whose_table.clone();
        let whose = Box::new(move |s: &std::os::unix::net::UnixStream| {
            let pid = peer_pid(std::os::fd::AsRawFd::as_raw_fd(s)).ok()?;
            vouched(&**t.get()?, pid).ok().map(|p| p.name)
        });
        let (d, w) = link(docket_driver_window::Window::new(c, whose)?);
        window_name = Some(written(&d.name()));
        drivers.push(d);
        waits.push(w);
    }
    // the clock, commits, sizes and proofs, and Git-Head and Sleep, as the file at --host says
    let mut host_name = None;
    // what the host's Pick sees, bound once the table and the engine are made
    let (seen_table, seen_engine) = (Rc::new(OnceCell::<Rc<Table>>::new()), Rc::new(OnceCell::<Rc<Engine>>::new()));
    if let Some(path) = host {
        let dir = std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new("."));
        let c = docket_driver_host::Config::read(&read(&path)?, dir).map_err(|e| format!("{path}: {e}"))?;
        let (t, e) = (seen_table.clone(), seen_engine.clone());
        let seen = docket_driver_host::Seen { live: Box::new(move || t.get().map_or_else(Vec::new, |t| t.live())), holds: Box::new(move |m, q| e.get().is_some_and(|e| e.holds(m, q))) };
        let (d, w) = link(docket_driver_host::Host::new(c, std::time::Duration::from_secs(60), seen)?);
        host_name = Some(written(&d.name()));
        drivers.push(d);
        waits.push(w);
    }
    // a model step's Effects, run by the command the file at --clerk names
    let mut clerk_name = None;
    if let Some(path) = clerk {
        let c: docket_driver_clerk::Config = serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        let (d, w) = link(docket_driver_clerk::Clerk::new(c, &loaded.syms).map_err(|e| format!("{path}: {e}"))?);
        clerk_name = Some(written(&d.name()));
        drivers.push(d);
        waits.push(w);
    }
    // a linked driver's name stands at its place
    let mut names = vec![hooks_name];
    names.extend(compute_name.into_iter().chain(window_name).chain(host_name).chain(clerk_name));
    if let Some(t) = &turns {
        let d = linked(t.clone());
        names.push(written(&d.name()));
        drivers.push(d);
        waits.push(t.clone());
    }
    // the drivers a party answers for over the socket, after the linked ones: a parties line names each by its name
    let hands = by_socket.map_or(Ok(Vec::new()), |p| read(&p).and_then(|t| Hand::read(&t)))?;
    let answered: Vec<usize> = (drivers.len()..drivers.len() + hands.len()).collect();
    for h in hands {
        names.push(h.name.clone());
        drivers.push(Box::new(h));
    }
    let table = Rc::new(Table::read(&read(&parties)?, &names)?);
    drop(seen_table.set(table.clone()));
    drop(whose_table.set(table.clone()));
    match starts {
        [Some(relay), Some(ps), Some(hooks), Some(homes), Some(spawn)] => {
            let places = Places { hooks: hooks.into(), homes: homes.into(), spawn: spawn.into(), carrier: hooks_at };
            let reads = turns.as_ref().map(|_| docket_driver_turns::Turns::reads as fn(&str) -> bool);
            let (s, c) = session(relay, profiles(&read(&ps)?)?, frames, table.clone(), places, reads);
            drivers.push(Box::new(s));
            *copy.borrow_mut() = Some(c);
        }
        [None, None, None, None, None] => {}
        _ => return Err(format!("a session's start wants all of --relay, --profiles, --hooks, --homes and --spawn, or none: {USAGE}")),
    }
    let mut runs = Machine::new(loaded, docket_v2::norm::Core::default(), drivers)?;
    runs.clock = Box::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64));
    runs.head = Box::new(coordd_v2::head::head);
    runs.put = Box::new(coordd_v2::head::put);
    let parties = table.clone();
    let bind = Box::new(move |_: &str, party: &docket_v2::run::value::Value| parties.named(&written(std::slice::from_ref(party))).map_or((None, String::new()), |p| (p.driver, p.folder)));
    // a place the record is also kept in: restored from before it opens, sent to as it grows
    let sending = match &record_place {
        Some(path) => {
            let dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new("."));
            let c = coordd_v2::sync::Config::read(&read(path)?, dir).map_err(|e| format!("{path}: {e}"))?;
            let (s, said) = coordd_v2::sync::begin(&c, record.as_ref()).map_err(|e| format!("{path}: {e}"))?;
            eprintln!("coordd-v2: record place: {said}");
            Some((s, c.delays()))
        }
        None => None,
    };
    let kept = PathBuf::from(&record);
    let docket = Docket::new(runs, &top, answered, bind, Box::new(move |h: &[u8; 32]| coordd_v2::texts::kept(&kept, h)));
    let engine = Engine::open(record.as_ref(), Box::new(docket), Box::new(table), Box::new(NoTerminals))?;
    drop(seen_engine.set(engine.clone()));
    // the place's guard is in before the first request is taken
    let keeper = sending.map(|(s, delays)| (coordd_v2::sync::Keeper::new(s, PathBuf::from(&record), coordd_v2::sync::guard(&engine)), delays));
    if std::fs::symlink_metadata(&socket).is_ok_and(|m| std::os::unix::fs::FileTypeExt::is_socket(&m.file_type())) {
        std::fs::remove_file(&socket).map_err(|e| format!("{socket}: {e}"))?;
    }
    // the machine is not Send: every task of the daemon runs on this one, in the local set
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, async move {
        let listener = tokio::net::UnixListener::bind(&socket).map_err(|e| format!("{socket}: {e}"))?;
        for (place, w) in waits.into_iter().enumerate() {
            tokio::task::spawn_local(wait_on(engine.clone(), w, place));
        }
        if let Some((k, (every, check))) = &keeper {
            tokio::task::spawn_local(k.clone().run(engine.clone(), *every, *check));
        }
        eprintln!("coordd-v2: serving {record} on {socket}");
        // at SIGTERM or SIGINT, what the record gained goes to its place before the daemon ends
        let (mut term, mut int) = (signal(SignalKind::terminate()).map_err(|e| e.to_string())?, signal(SignalKind::interrupt()).map_err(|e| e.to_string())?);
        tokio::select! {
            r = engine.clone().serve(listener) => r.map_err(|e| e.to_string()),
            _ = term.recv() => Ok(()),
            _ = int.recv() => Ok(()),
        }?;
        if let Some((k, _)) = &keeper {
            eprintln!("coordd-v2: record place at shutdown: {}", k.flush().await);
        }
        Ok(())
    })
}
